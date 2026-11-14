//! Windows Media Foundation H.265 / HEVC Hardware Codec Engine
//!
//! Provides GPU-accelerated H.265 / HEVC video encoding and decoding via Windows Media Foundation:
//! - Supported on Windows 10 & 11 GPUs with NVIDIA NVENC (Pascal+), Intel QuickSync (Skylake+), AMD AMF.
//! - Uses native `MFVideoFormat_HEVC` subtype.
//! - Automatically yields to H.264 or pure-Rust SIMD Motion-JPEG if unavailable.

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use windows::core::Interface;

use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, IMFTransform, IMF2DBuffer, MFMediaType_Video, MFT_CATEGORY_VIDEO_DECODER,
    MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER,
    MFT_ENUM_FLAG_SYNCMFT, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER, MFT_REGISTER_TYPE_INFO,
    MFVideoFormat_HEVC, MFVideoFormat_NV12, MF_E_TRANSFORM_NEED_MORE_INPUT,
    MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_MT_DEFAULT_STRIDE, MF_TRANSFORM_ASYNC_UNLOCK,
};

use super::h264_mf::{
    bgra_to_nv12, ensure_mf_initialized, get_mfplat, nv12_to_bgra_strided, MfPlatSymbols,
};
use crate::codec::{CodecType, EncodedFrame, VideoDecoder, VideoEncoder};

static H265_AVAILABLE: AtomicBool = AtomicBool::new(false);
static H265_CHECKED: AtomicBool = AtomicBool::new(false);

/// Probes whether the current system has a hardware-accelerated H.265 / HEVC encoder
pub fn is_h265_hardware_available() -> bool {
    if H265_CHECKED.load(Ordering::Relaxed) {
        return H265_AVAILABLE.load(Ordering::Relaxed);
    }

    if ensure_mf_initialized().is_err() {
        H265_CHECKED.store(true, Ordering::Relaxed);
        H265_AVAILABLE.store(false, Ordering::Relaxed);
        return false;
    }

    let mf = match get_mfplat() {
        Some(m) => m,
        None => {
            H265_CHECKED.store(true, Ordering::Relaxed);
            H265_AVAILABLE.store(false, Ordering::Relaxed);
            return false;
        }
    };

    let has_hevc = unsafe {
        let output_type = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_HEVC,
        };

        let mut activate_ptrs = std::ptr::null_mut();
        let mut count = 0u32;

        let res = mf.mft_enum_ex(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
            None,
            Some(&output_type),
            &mut activate_ptrs,
            &mut count,
        );

        let mut sync_enc_found = false;
        if res.is_ok() && count > 0 && !activate_ptrs.is_null() {
            let activations = std::slice::from_raw_parts(activate_ptrs, count as usize);
            for act_opt in activations {
                if let Some(act) = act_opt {
                    let is_async = act.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_TRANSFORM_ASYNC).unwrap_or(0);
                    if is_async == 0 {
                        sync_enc_found = true;
                        break;
                    }
                }
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(activate_ptrs as *const _));
        }

        // Also verify that an H.265 decoder is installed on the system (Microsoft HEVC extension)
        let mut dec_found = false;
        if sync_enc_found {
            let input_type = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_HEVC,
            };
            let mut dec_ptrs = std::ptr::null_mut();
            let mut dec_count = 0u32;
            let dec_res = mf.mft_enum_ex(
                windows::Win32::Media::MediaFoundation::MFT_CATEGORY_VIDEO_DECODER,
                MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
                Some(&input_type),
                None,
                &mut dec_ptrs,
                &mut dec_count,
            );
            if dec_res.is_ok() && dec_count > 0 && !dec_ptrs.is_null() {
                dec_found = true;
                windows::Win32::System::Com::CoTaskMemFree(Some(dec_ptrs as *const _));
            }
        }

        sync_enc_found && dec_found
    };

    info!(
        "Windows Media Foundation H.265 (HEVC) status: {}",
        if has_hevc { "AVAILABLE (Hardware Accelerated)" } else { "NOT EXTENDED / NOT AVAILABLE" }
    );

    H265_AVAILABLE.store(has_hevc, Ordering::Relaxed);
    H265_CHECKED.store(true, Ordering::Relaxed);
    has_hevc
}

/// RAII guard ensuring MFT_OUTPUT_DATA_BUFFER COM interface members (pSample, pEvents)
/// are always released, completely eliminating memory accumulation over time.
pub(crate) struct MftOutputGuard<'a> {
    pub(crate) data: &'a mut MFT_OUTPUT_DATA_BUFFER,
}

impl<'a> Drop for MftOutputGuard<'a> {
    fn drop(&mut self) {
        unsafe {
            core::mem::ManuallyDrop::drop(&mut self.data.pSample);
            core::mem::ManuallyDrop::drop(&mut self.data.pEvents);
        }
    }
}

/// Windows Media Foundation Hardware H.265 / HEVC Video Encoder
pub struct H265MfEncoder {
    width: u32,
    height: u32,
    bitrate: u32,
    fps: u32,
    transform: Option<IMFTransform>,
    frame_index: u64,
    mf: Arc<MfPlatSymbols>,
    nv12_scratch: Vec<u8>,
    out_buf_size: u32,
}

unsafe impl Send for H265MfEncoder {}

impl H265MfEncoder {
    pub fn new(width: u32, height: u32, fps: u32, bitrate: u32) -> Result<Self> {
        ensure_mf_initialized()?;
        let mf = get_mfplat().ok_or_else(|| anyhow!("mfplat.dll not loaded"))?;
        let nv12_len = (width * height * 3 / 2) as usize;

        let mut encoder = Self {
            width,
            height,
            bitrate: bitrate.clamp(500_000, 20_000_000),
            fps: fps.clamp(15, 120),
            transform: None,
            frame_index: 0,
            mf,
            nv12_scratch: vec![0u8; nv12_len],
            out_buf_size: (width * height) * 2,
        };

        if let Err(e) = encoder.init_transform() {
            warn!("Failed to initialize Windows H.265 MFT encoder (falling back to H.264/MJPEG): {:?}", e);
            return Err(e);
        }

        Ok(encoder)
    }

    fn init_transform(&mut self) -> Result<()> {
        unsafe {
            let output_type = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_HEVC,
            };

            let mut activate_ptrs = std::ptr::null_mut();
            let mut count = 0u32;

            let res = self.mf.mft_enum_ex(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
                None,
                Some(&output_type),
                &mut activate_ptrs,
                &mut count,
            );

            res?;

            if count == 0 || activate_ptrs.is_null() {
                return Err(anyhow!("No H.265 MFT video encoder found on this system"));
            }

            let activations = std::slice::from_raw_parts(activate_ptrs, count as usize);
            let mut chosen_transform: Option<IMFTransform> = None;
            let mut chosen_is_async = false;
            for act_opt in activations {
                if let Some(act) = act_opt {
                    let is_async = act.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_TRANSFORM_ASYNC).unwrap_or(0);
                    if is_async == 0 {
                        if let Ok(trans) = act.ActivateObject::<IMFTransform>() {
                            chosen_transform = Some(trans);
                            chosen_is_async = false;
                            break;
                        }
                    }
                }
            }
            if chosen_transform.is_none() {
                for act_opt in activations {
                    if let Some(act) = act_opt {
                        if let Ok(trans) = act.ActivateObject::<IMFTransform>() {
                            chosen_transform = Some(trans);
                            chosen_is_async = true;
                            break;
                        }
                    }
                }
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(activate_ptrs as *const _));

            let transform = chosen_transform.ok_or_else(|| anyhow!("Failed to activate H.265 MFT encoder"))?;

            if chosen_is_async {
                if let Ok(attrs) = transform.GetAttributes() {
                    let _ = attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1);
                }
            }

            let out_media: IMFMediaType = self.mf.create_media_type()?;
            out_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_HEVC)?;
            out_media.SetUINT32(&MF_MT_AVG_BITRATE, self.bitrate)?;

            let frame_rate = ((self.fps as u64) << 32) | 1u64;
            out_media.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;

            let frame_size = ((self.width as u64) << 32) | (self.height as u64);
            out_media.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            out_media.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;

            transform.SetOutputType(0, &out_media, 0)?;

            let in_media: IMFMediaType = self.mf.create_media_type()?;
            in_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            in_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            in_media.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            in_media.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;
            in_media.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;

            transform.SetInputType(0, &in_media, 0)?;

            let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
            let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);

            if let Ok(stream_info) = transform.GetOutputStreamInfo(0) {
                self.out_buf_size = stream_info.cbSize.max(65536);
            }

            self.transform = Some(transform);
            info!("H.265 MFT Encoder initialized for {}x{} @ {} FPS ({} kbps)", self.width, self.height, self.fps, self.bitrate / 1000);
            Ok(())
        }
    }
}

impl VideoEncoder for H265MfEncoder {
    fn codec_type(&self) -> CodecType {
        CodecType::H265
    }

    fn encode(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        _force_keyframe: bool,
    ) -> Result<EncodedFrame> {
        let transform = self.transform.as_ref().ok_or_else(|| anyhow!("H265 encoder transform uninitialized"))?;

        if width != self.width || height != self.height {
            return Err(anyhow!("Resolution changed during H265 session: expected {}x{}, got {}x{}", self.width, self.height, width, height));
        }

        let nv12_len = (width * height * 3 / 2) as usize;
        if self.nv12_scratch.len() != nv12_len {
            self.nv12_scratch.resize(nv12_len, 0);
        }
        bgra_to_nv12(bgra, width as usize, height as usize, &mut self.nv12_scratch);

        unsafe {
            let buffer = self.mf.create_memory_buffer(nv12_len as u32)?;
            let mut ptr = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None)?;
            std::ptr::copy_nonoverlapping(self.nv12_scratch.as_ptr(), ptr, nv12_len);
            buffer.Unlock()?;
            buffer.SetCurrentLength(nv12_len as u32)?;

            let sample = self.mf.create_sample()?;
            sample.AddBuffer(&buffer)?;

            let duration = (10_000_000u64 / self.fps as u64) as i64;
            let time = (self.frame_index * (10_000_000u64 / self.fps as u64)) as i64;
            self.frame_index += 1;

            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(duration)?;

            transform.ProcessInput(0, &sample, 0)?;

            let out_mem = self.mf.create_memory_buffer(self.out_buf_size)?;
            let out_sample = self.mf.create_sample()?;
            out_sample.AddBuffer(&out_mem)?;

            let mut output_data = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: core::mem::ManuallyDrop::new(Some(out_sample.clone())),
                dwStatus: 0,
                pEvents: core::mem::ManuallyDrop::new(None),
            };
            let guard = MftOutputGuard { data: &mut output_data };

            let mut status = 0u32;
            let process_res = transform.ProcessOutput(0, std::slice::from_mut(guard.data), &mut status);

            if let Err(e) = process_res {
                drop(guard);
                if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                    return Ok(EncodedFrame {
                        codec: CodecType::H265,
                        is_keyframe: false,
                        data: Vec::new(),
                        orig_width: width,
                        orig_height: height,
                    });
                }
                return Err(anyhow!("MFT HEVC ProcessOutput failed: {:?}", e));
            }

            let mut out_bytes = Vec::new();
            if let Ok(m_buf) = out_sample.ConvertToContiguousBuffer() {
                let mut data_ptr = std::ptr::null_mut();
                let mut max_l = 0u32;
                let mut cur_l = 0u32;
                m_buf.Lock(&mut data_ptr, Some(&mut max_l), Some(&mut cur_l))?;
                if cur_l > 0 && !data_ptr.is_null() {
                    out_bytes = std::slice::from_raw_parts(data_ptr, cur_l as usize).to_vec();
                }
                m_buf.Unlock()?;
            }

            drop(guard);

            let is_keyframe = if out_bytes.len() > 4 {
                out_bytes.windows(4).any(|w| w == [0, 0, 0, 1] || w[0..3] == [0, 0, 1])
            } else {
                false
            };

            Ok(EncodedFrame {
                codec: CodecType::H265,
                is_keyframe,
                data: out_bytes,
                orig_width: width,
                orig_height: height,
            })
        }
    }
}

/// Windows Media Foundation Hardware H.265 / HEVC Video Decoder
pub struct H265MfDecoder {
    width: u32,
    height: u32,
    transform: Option<IMFTransform>,
    mf: Option<Arc<MfPlatSymbols>>,
    out_buf_size: u32,
    frame_index: u64,
    last_frame: Option<Vec<u8>>,
}

unsafe impl Send for H265MfDecoder {}

impl H265MfDecoder {
    pub fn new() -> Result<Self> {
        ensure_mf_initialized()?;
        let mf = get_mfplat().ok_or_else(|| anyhow!("mfplat.dll not loaded"))?;
        Ok(Self {
            width: 0,
            height: 0,
            transform: None,
            mf: Some(mf),
            out_buf_size: 1_728_000,
            frame_index: 0,
            last_frame: None,
        })
    }

    fn init_decoder(&mut self, width: u32, height: u32) -> Result<()> {
        let mf = self.mf.as_ref().ok_or_else(|| anyhow!("mfplat not available"))?;
        unsafe {
            let in_type = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_HEVC,
            };
            let mut activate_ptrs = std::ptr::null_mut();
            let mut count = 0u32;
            let mut res = mf.mft_enum_ex(
                MFT_CATEGORY_VIDEO_DECODER,
                MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
                Some(&in_type),
                None,
                &mut activate_ptrs,
                &mut count,
            );
            if res.is_err() || count == 0 || activate_ptrs.is_null() {
                res = mf.mft_enum_ex(
                    MFT_CATEGORY_VIDEO_DECODER,
                    MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
                    Some(&in_type),
                    None,
                    &mut activate_ptrs,
                    &mut count,
                );
            }
            res?;
            if count == 0 || activate_ptrs.is_null() {
                return Err(anyhow!("No H.265 MFT video decoder found on this system"));
            }
            let activations = std::slice::from_raw_parts(activate_ptrs, count as usize);
            let mut chosen_transform: Option<IMFTransform> = None;
            for act_opt in activations {
                if let Some(act) = act_opt {
                    let is_async = act.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_TRANSFORM_ASYNC).unwrap_or(0);
                    if is_async == 0 {
                        if let Ok(trans) = act.ActivateObject::<IMFTransform>() {
                            chosen_transform = Some(trans);
                            break;
                        }
                    }
                }
            }
            if chosen_transform.is_none() {
                for act_opt in activations {
                    if let Some(act) = act_opt {
                        if let Ok(trans) = act.ActivateObject::<IMFTransform>() {
                            chosen_transform = Some(trans);
                            break;
                        }
                    }
                }
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(activate_ptrs as *const _));

            let transform = chosen_transform.ok_or_else(|| anyhow!("No H.265 MFT video decoder could be activated"))?;

            let frame_size = ((width as u64) << 32) | (height as u64);
            let frame_rate = (60u64 << 32) | 1u64;

            let in_media: IMFMediaType = mf.create_media_type()?;
            in_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            in_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_HEVC)?;
            in_media.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            in_media.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;
            transform.SetInputType(0, &in_media, 0)?;

            let out_media: IMFMediaType = mf.create_media_type()?;
            out_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            out_media.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            out_media.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;
            transform.SetOutputType(0, &out_media, 0)?;

            let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
            let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);

            if let Ok(stream_info) = transform.GetOutputStreamInfo(0) {
                self.out_buf_size = stream_info.cbSize.max((width * height * 3 / 2) as u32);
            }

            self.transform = Some(transform);
            self.width = width;
            self.height = height;
            self.frame_index = 0;
            self.last_frame = None;
            info!("H.265 Media Foundation Decoder initialized for {}x{}", width, height);
            Ok(())
        }
    }

    fn try_pull_output(
        &mut self,
        expected_width: u32,
        expected_height: u32,
    ) -> Option<Vec<u8>> {
        let transform = self.transform.as_ref()?;
        let mf = self.mf.as_ref()?;
        unsafe {
            let out_mem = mf.create_memory_buffer(self.out_buf_size).ok()?;
            let out_sample = mf.create_sample().ok()?;
            out_sample.AddBuffer(&out_mem).ok()?;

            let mut output_data = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: core::mem::ManuallyDrop::new(Some(out_sample.clone())),
                dwStatus: 0,
                pEvents: core::mem::ManuallyDrop::new(None),
            };
            let guard = MftOutputGuard { data: &mut output_data };

            let mut status = 0u32;
            let mut res = transform.ProcessOutput(0, std::slice::from_mut(guard.data), &mut status);

            // Handle MF_E_TRANSFORM_STREAM_CHANGE (0xC00D6D61) - SPS/PPS renegotiation
            if let Err(ref e) = res {
                if e.code().0 as u32 == 0xC00D6D61 || (status & 0x100 != 0) {
                    if let Ok(new_out_type) = transform.GetOutputAvailableType(0, 0) {
                        let _ = transform.SetOutputType(0, &new_out_type, 0);
                        if let Ok(info) = transform.GetOutputStreamInfo(0) {
                            self.out_buf_size = info.cbSize.max((expected_width * expected_height * 3 / 2) as u32);
                        }
                    }
                    if let Ok(new_mem) = mf.create_memory_buffer(self.out_buf_size) {
                        if let Ok(new_samp) = mf.create_sample() {
                            let _ = new_samp.AddBuffer(&new_mem);
                            core::mem::ManuallyDrop::drop(&mut guard.data.pSample);
                            guard.data.pSample = core::mem::ManuallyDrop::new(Some(new_samp.clone()));
                            res = transform.ProcessOutput(0, std::slice::from_mut(guard.data), &mut status);
                        }
                    }
                }
            }

            if res.is_ok() {
                let active_sample = guard.data.pSample.as_ref().cloned().unwrap_or(out_sample);
                if let Ok(m_buf) = active_sample.ConvertToContiguousBuffer() {
                    let mut dec_w = expected_width;
                    let mut dec_h = expected_height;
                    let mut dec_stride = expected_width;

                    if let Ok(cur_out) = transform.GetOutputCurrentType(0) {
                        if let Ok(fs) = cur_out.GetUINT64(&MF_MT_FRAME_SIZE) {
                            let w = (fs >> 32) as u32;
                            let h = (fs & 0xFFFF_FFFF) as u32;
                            if w > 0 && h > 0 {
                                dec_w = w;
                                dec_h = h;
                            }
                        }
                        if let Ok(st) = cur_out.GetUINT32(&MF_MT_DEFAULT_STRIDE) {
                            if st > 0 {
                                dec_stride = st;
                            }
                        }
                    }

                    // Attempt 1: Try IMF2DBuffer::ContiguousCopyTo
                    let mut packed_nv12: Option<Vec<u8>> = None;
                    if let Ok(buf2d) = m_buf.cast::<IMF2DBuffer>() {
                        if let Ok(contig_len) = buf2d.GetContiguousLength() {
                            if contig_len > 0 {
                                let mut dest = vec![0u8; contig_len as usize];
                                if buf2d.ContiguousCopyTo(&mut dest).is_ok() {
                                    packed_nv12 = Some(dest);
                                }
                            }
                        }
                    }

                    let bgra_len = (expected_width * expected_height * 4) as usize;
                    let mut bgra = vec![0u8; bgra_len];

                    if let Some(ref nv12) = packed_nv12 {
                        nv12_to_bgra_strided(
                            nv12,
                            expected_width as usize,
                            expected_height as usize,
                            dec_w as usize,
                            dec_h as usize,
                            &mut bgra,
                        );
                        self.last_frame = Some(bgra.clone());
                        return Some(bgra);
                    } else {
                        let mut data_ptr = std::ptr::null_mut();
                        let mut max_l = 0u32;
                        let mut cur_l = 0u32;
                        if m_buf.Lock(&mut data_ptr, Some(&mut max_l), Some(&mut cur_l)).is_ok() {
                            if cur_l > 0 && !data_ptr.is_null() {
                                let nv12_slice = std::slice::from_raw_parts(data_ptr, cur_l as usize);
                                nv12_to_bgra_strided(
                                    nv12_slice,
                                    expected_width as usize,
                                    expected_height as usize,
                                    dec_stride as usize,
                                    dec_h as usize,
                                    &mut bgra,
                                );
                                let _ = m_buf.Unlock();
                                self.last_frame = Some(bgra.clone());
                                return Some(bgra);
                            }
                            let _ = m_buf.Unlock();
                        }
                    }
                }
            }
            None
        }
    }
}

impl Default for H265MfDecoder {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            width: 0,
            height: 0,
            transform: None,
            mf: None,
            out_buf_size: 1_728_000,
            frame_index: 0,
            last_frame: None,
        })
    }
}

impl VideoDecoder for H265MfDecoder {
    fn codec_type(&self) -> CodecType {
        CodecType::H265
    }

    fn decode(
        &mut self,
        data: &[u8],
        expected_width: u32,
        expected_height: u32,
    ) -> Result<Vec<u8>> {
        if self.transform.is_none() || self.width != expected_width || self.height != expected_height {
            self.init_decoder(expected_width, expected_height)?;
        }
        let transform = self.transform.clone().ok_or_else(|| anyhow!("Decoder transform uninitialized"))?;
        let mf = self.mf.clone().ok_or_else(|| anyhow!("mfplat not loaded"))?;

        unsafe {
            let in_buf = mf.create_memory_buffer(data.len() as u32)?;
            let mut ptr = std::ptr::null_mut();
            in_buf.Lock(&mut ptr, None, None)?;
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            in_buf.Unlock()?;
            in_buf.SetCurrentLength(data.len() as u32)?;

            let sample = mf.create_sample()?;
            sample.AddBuffer(&in_buf)?;
            let time = (self.frame_index * (10_000_000 / 60)) as i64;
            self.frame_index += 1;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(166_666)?;

            let mut inp_res = transform.ProcessInput(0, &sample, 0);
            if let Err(ref e) = inp_res {
                if e.code() == windows::Win32::Media::MediaFoundation::MF_E_NOTACCEPTING {
                    let _ = self.try_pull_output(expected_width, expected_height);
                    inp_res = transform.ProcessInput(0, &sample, 0);
                }
            }
            let _ = inp_res;

            if let Some(bgra) = self.try_pull_output(expected_width, expected_height) {
                return Ok(bgra);
            }

            if let Some(ref last) = self.last_frame {
                return Ok(last.clone());
            }

            Err(anyhow!("H.265 decoder needs more input"))
        }
    }
}
