//! Windows Media Foundation Hardware H.264 Codec Engine
//!
//! Directly invokes Windows built-in GPU encoder/decoder MFTs (NVIDIA NVENC,
//! Intel QuickSync, AMD AMF, and Microsoft H.264 MFT) via native Win32 COM interfaces.
//!
//! DYNAMIC RUNTIME LOADING:
//! - Functions (`MFStartup`, `MFTEnumEx`, `MFCreateMediaType`, etc.) are resolved
//!   dynamically via `LoadLibraryW(w!("mfplat.dll"))` and `GetProcAddress`.
//! - ZERO static import of `mfplat.dll` in the PE Import Address Table (IAT).
//! - 100% crash-free compatibility across ALL Windows releases:
//!   Windows 11, 10, 8.1, 8, 7 SP1, Windows Server (2008 R2, 2012, 2016, 2019, 2022, 2025,
//!   including Server Core without Media Foundation), and Windows N/KN editions.
//! - If `mfplat.dll` is absent or hardware MFT is unsupported, it gracefully yields to
//!   the pure-Rust SIMD Motion-JPEG 4:4:4 engine.

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tracing::{debug, info, warn};

use windows::core::{w, Interface};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFMediaBuffer, IMFMediaType, IMFSample, IMFTransform, IMF2DBuffer,
    MFMediaType_Video, MFT_CATEGORY_VIDEO_DECODER, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_REGISTER_TYPE_INFO, MFVideoFormat_H264, MFVideoFormat_NV12,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE,
    MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_MT_DEFAULT_STRIDE,
    MFSTARTUP_NOSOCKET, MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
};

use crate::codec::{CodecType, EncodedFrame, VideoDecoder, VideoEncoder};

static MF_INITIALIZED: AtomicBool = AtomicBool::new(false);
static H264_HW_CHECKED: AtomicBool = AtomicBool::new(false);
static H264_HW_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Dynamic function pointers loaded from `mfplat.dll` at runtime
pub(crate) struct MfPlatSymbols {
    pub(crate) _hmodule: HMODULE,
    pub(crate) startup: unsafe extern "system" fn(u32, u32) -> windows::core::HRESULT,
    pub(crate) create_media_type: unsafe extern "system" fn(*mut *mut core::ffi::c_void) -> windows::core::HRESULT,
    pub(crate) create_memory_buffer: unsafe extern "system" fn(u32, *mut *mut core::ffi::c_void) -> windows::core::HRESULT,
    pub(crate) create_sample: unsafe extern "system" fn(*mut *mut core::ffi::c_void) -> windows::core::HRESULT,
    pub(crate) mft_enum_ex: unsafe extern "system" fn(
        windows::core::GUID,
        i32,
        *const MFT_REGISTER_TYPE_INFO,
        *const MFT_REGISTER_TYPE_INFO,
        *mut *mut *mut core::ffi::c_void,
        *mut u32,
    ) -> windows::core::HRESULT,
}

unsafe impl Send for MfPlatSymbols {}
unsafe impl Sync for MfPlatSymbols {}

static MF_SYMBOLS: OnceLock<Option<Arc<MfPlatSymbols>>> = OnceLock::new();

pub(crate) fn get_mfplat() -> Option<Arc<MfPlatSymbols>> {
    MF_SYMBOLS
        .get_or_init(|| unsafe {
            let hmod = match LoadLibraryW(w!("mfplat.dll")) {
                Ok(h) if !h.is_invalid() => h,
                _ => {
                    debug!("Windows Media Foundation: mfplat.dll not available on this Windows edition");
                    return None;
                }
            };

            let p_startup = GetProcAddress(hmod, windows::core::s!("MFStartup"))?;
            let p_create_mt = GetProcAddress(hmod, windows::core::s!("MFCreateMediaType"))?;
            let p_create_mb = GetProcAddress(hmod, windows::core::s!("MFCreateMemoryBuffer"))?;
            let p_create_sm = GetProcAddress(hmod, windows::core::s!("MFCreateSample"))?;
            let p_enum_ex = GetProcAddress(hmod, windows::core::s!("MFTEnumEx"))?;

            Some(Arc::new(MfPlatSymbols {
                _hmodule: hmod,
                startup: std::mem::transmute(p_startup),
                create_media_type: std::mem::transmute(p_create_mt),
                create_memory_buffer: std::mem::transmute(p_create_mb),
                create_sample: std::mem::transmute(p_create_sm),
                mft_enum_ex: std::mem::transmute(p_enum_ex),
            }))
        })
        .clone()
}

impl MfPlatSymbols {
    pub(crate) unsafe fn create_media_type(&self) -> windows::core::Result<IMFMediaType> {
        let mut result__ = unsafe { core::mem::zeroed() };
        unsafe {
            (self.create_media_type)(&mut result__).and_then(|| windows::core::Type::from_abi(result__))
        }
    }

    pub(crate) unsafe fn create_memory_buffer(&self, len: u32) -> windows::core::Result<IMFMediaBuffer> {
        let mut result__ = unsafe { core::mem::zeroed() };
        unsafe {
            (self.create_memory_buffer)(len, &mut result__).and_then(|| windows::core::Type::from_abi(result__))
        }
    }

    pub(crate) unsafe fn create_sample(&self) -> windows::core::Result<IMFSample> {
        let mut result__ = unsafe { core::mem::zeroed() };
        unsafe {
            (self.create_sample)(&mut result__).and_then(|| windows::core::Type::from_abi(result__))
        }
    }

    pub(crate) unsafe fn mft_enum_ex(
        &self,
        category: windows::core::GUID,
        flags: i32,
        input_type: Option<&MFT_REGISTER_TYPE_INFO>,
        output_type: Option<&MFT_REGISTER_TYPE_INFO>,
        activate_ptrs: *mut *mut Option<IMFActivate>,
        count: *mut u32,
    ) -> windows::core::Result<()> {
        let in_ptr = input_type.map_or(std::ptr::null(), |t| t as *const _);
        let out_ptr = output_type.map_or(std::ptr::null(), |t| t as *const _);
        unsafe {
            (self.mft_enum_ex)(category, flags, in_ptr, out_ptr, activate_ptrs as _, count).ok()
        }
    }
}

/// Initializes the Windows Media Foundation subsystem if not already running
pub fn ensure_mf_initialized() -> Result<()> {
    if !MF_INITIALIZED.load(Ordering::SeqCst) {
        let mf = get_mfplat().ok_or_else(|| anyhow!("Windows Media Foundation (mfplat.dll) is not available on this system"))?;
        unsafe {
            (mf.startup)(MF_VERSION, MFSTARTUP_NOSOCKET).ok()?;
        }
        MF_INITIALIZED.store(true, Ordering::SeqCst);
        info!("Windows Media Foundation subsystem successfully initialized");
    }
    Ok(())
}

/// Probes whether the current system has a hardware-accelerated H.264 encoder (NVIDIA/Intel/AMD)
pub fn is_h264_hardware_available() -> bool {
    if H264_HW_CHECKED.load(Ordering::Relaxed) {
        return H264_HW_AVAILABLE.load(Ordering::Relaxed);
    }

    let mf = match get_mfplat() {
        Some(m) => m,
        None => {
            H264_HW_CHECKED.store(true, Ordering::Relaxed);
            H264_HW_AVAILABLE.store(false, Ordering::Relaxed);
            info!("Windows Media Foundation: mfplat.dll absent (Windows Server / N edition / minimal install); using Motion-JPEG fallback");
            return false;
        }
    };

    if ensure_mf_initialized().is_err() {
        H264_HW_CHECKED.store(true, Ordering::Relaxed);
        H264_HW_AVAILABLE.store(false, Ordering::Relaxed);
        return false;
    }

    let has_hw = unsafe {
        let output_type = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
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

        let mut sync_found = false;
        if res.is_ok() && count > 0 && !activate_ptrs.is_null() {
            let activations = std::slice::from_raw_parts(activate_ptrs, count as usize);
            for act_opt in activations {
                if let Some(act) = act_opt {
                    let is_async = act.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_TRANSFORM_ASYNC).unwrap_or(0);
                    if is_async == 0 {
                        sync_found = true;
                        break;
                    }
                }
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(activate_ptrs as *const _));
        }

        sync_found
    };

    info!(
        "Windows Media Foundation H.264 Hardware status: {}",
        if has_hw { "AVAILABLE (Media Foundation H.264 Encoder)" } else { "UNAVAILABLE (Will use Motion-JPEG fallback)" }
    );

    H264_HW_AVAILABLE.store(has_hw, Ordering::Relaxed);
    H264_HW_CHECKED.store(true, Ordering::Relaxed);
    has_hw
}

/// Converts a BGRA buffer to NV12 (Y plane followed by interleaved UV plane).
pub fn bgra_to_nv12(bgra: &[u8], width: usize, height: usize, nv12: &mut [u8]) {
    let y_plane_len = width * height;
    let uv_plane_len = width * (height / 2);
    if nv12.len() < y_plane_len + uv_plane_len {
        return;
    }
    let (y_plane, uv_plane) = nv12.split_at_mut(y_plane_len);

    // Compute Y plane
    for y in 0..height {
        let row_start = y * width;
        let bgra_row_start = row_start * 4;
        for x in 0..width {
            let px = bgra_row_start + x * 4;
            if px + 2 >= bgra.len() || row_start + x >= y_plane.len() {
                break;
            }
            let b = bgra[px] as i32;
            let g = bgra[px + 1] as i32;
            let r = bgra[px + 2] as i32;
            let y_val = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
            y_plane[row_start + x] = y_val.clamp(16, 235) as u8;
        }
    }

    // Compute interleaved UV plane (subsampled 2x2)
    let half_h = height / 2;
    let half_w = width / 2;
    for y2 in 0..half_h {
        let y = y2 * 2;
        let uv_row_start = y2 * width;
        let bgra_row_start = y * width * 4;
        for x2 in 0..half_w {
            let x = x2 * 2;
            let px = bgra_row_start + x * 4;
            let uv_idx = uv_row_start + x2 * 2;
            if px + 2 >= bgra.len() || uv_idx + 1 >= uv_plane.len() {
                break;
            }
            let b = bgra[px] as i32;
            let g = bgra[px + 1] as i32;
            let r = bgra[px + 2] as i32;
            let u_val = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
            let v_val = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
            uv_plane[uv_idx] = u_val.clamp(16, 240) as u8;
            uv_plane[uv_idx + 1] = v_val.clamp(16, 240) as u8;
        }
    }
}

/// Converts an NV12 buffer (Y plane followed by interleaved UV plane) with stride and macroblock alignment to BGRA.
pub fn nv12_to_bgra_strided(
    nv12: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    coded_h: usize,
    bgra: &mut [u8],
) {
    let row_stride = stride.max(width);
    let bytes_per_row_yuv = row_stride * 3 / 2;

    // Accurately determine the actual height of the Y plane based on buffer size and metadata.
    // In NV12, a Y plane of H rows followed by a UV plane of H/2 rows requires exactly row_stride * H * 1.5 bytes.
    let actual_h = if bytes_per_row_yuv > 0 {
        let max_possible_h = nv12.len() / bytes_per_row_yuv;
        if nv12.len() == row_stride * height * 3 / 2 {
            // Buffer is tightly packed for the visible image height (e.g. from ContiguousCopyTo)
            height
        } else if coded_h >= height && nv12.len() >= row_stride * coded_h * 3 / 2 {
            // Buffer has allocated macroblock rows matching coded_h
            coded_h
        } else if max_possible_h >= height {
            max_possible_h
        } else {
            height
        }
    } else {
        height
    };

    let y_plane_len = row_stride * actual_h;
    if nv12.len() < y_plane_len {
        return;
    }
    let y_plane = &nv12[..y_plane_len];
    let uv_plane = &nv12[y_plane_len..];

    for y in 0..height {
        let y_row_start = y * row_stride;
        let uv_row_start = (y / 2) * row_stride;
        let bgra_row_start = y * width * 4;

        if y_row_start + width > y_plane.len()
            || uv_row_start + width > uv_plane.len()
            || bgra_row_start + width * 4 > bgra.len()
        {
            break;
        }

        let y_row = &y_plane[y_row_start..y_row_start + width];
        let uv_row = &uv_plane[uv_row_start..uv_row_start + width];
        let bgra_row = &mut bgra[bgra_row_start..bgra_row_start + width * 4];

        for x in 0..width {
            let y_val = y_row[x] as i32;
            let uv_idx = (x / 2) * 2;
            let u_val = uv_row[uv_idx] as i32;
            let v_val = uv_row[uv_idx + 1] as i32;

            let c = y_val - 16;
            let d = u_val - 128;
            let e = v_val - 128;

            let r = ((298 * c + 409 * e + 128) >> 8).clamp(0, 255) as u8;
            let g = ((298 * c - 100 * d - 208 * e + 128) >> 8).clamp(0, 255) as u8;
            let b = ((298 * c + 516 * d + 128) >> 8).clamp(0, 255) as u8;

            let px = x * 4;
            bgra_row[px] = b;
            bgra_row[px + 1] = g;
            bgra_row[px + 2] = r;
            bgra_row[px + 3] = 255;
        }
    }
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

pub struct H264MfEncoder {
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

unsafe impl Send for H264MfEncoder {}

impl H264MfEncoder {
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
            warn!("Failed to initialize Windows H.264 MFT encoder (falling back to MJPEG): {:?}", e);
            return Err(e);
        }

        Ok(encoder)
    }

    fn init_transform(&mut self) -> Result<()> {
        unsafe {
            let output_type = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_H264,
            };

            let mut activate_ptrs = std::ptr::null_mut();
            let mut count = 0u32;

            let res = self.mf.mft_enum_ex(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
                None,
                Some(&output_type),
                &mut activate_ptrs,
                &mut count,
            );

            res?;

            if count == 0 || activate_ptrs.is_null() {
                return Err(anyhow!("No H.264 MFT video encoder found on this system"));
            }

            let activations = std::slice::from_raw_parts(activate_ptrs, count as usize);
            let mut chosen_transform: Option<IMFTransform> = None;
            let mut chosen_is_async = false;
            let mut chosen_name = String::new();

            // Pass 1: Prioritize synchronous encoders (async == 0) to avoid GPU driver crashes
            for act_opt in activations {
                if let Some(act) = act_opt {
                    let is_async = act.GetUINT32(&windows::Win32::Media::MediaFoundation::MF_TRANSFORM_ASYNC).unwrap_or(0);
                    if is_async == 0 {
                        if let Ok(trans) = act.ActivateObject::<IMFTransform>() {
                            let mut name_buf = [0u16; 128];
                            let mut name_len = 0u32;
                            let _ = act.GetString(&windows::Win32::Media::MediaFoundation::MFT_FRIENDLY_NAME_Attribute, &mut name_buf, Some(&mut name_len));
                            chosen_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                            chosen_transform = Some(trans);
                            chosen_is_async = false;
                            break;
                        }
                    }
                }
            }

            // Pass 2: Fallback to any encoder if no sync encoder succeeded
            if chosen_transform.is_none() {
                for act_opt in activations {
                    if let Some(act) = act_opt {
                        if let Ok(trans) = act.ActivateObject::<IMFTransform>() {
                            let mut name_buf = [0u16; 128];
                            let mut name_len = 0u32;
                            let _ = act.GetString(&windows::Win32::Media::MediaFoundation::MFT_FRIENDLY_NAME_Attribute, &mut name_buf, Some(&mut name_len));
                            chosen_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                            chosen_transform = Some(trans);
                            chosen_is_async = true;
                            break;
                        }
                    }
                }
            }

            windows::Win32::System::Com::CoTaskMemFree(Some(activate_ptrs as *const _));

            let transform = chosen_transform.ok_or_else(|| anyhow!("Failed to activate H.264 MFT encoder"))?;
            info!("Selected H.264 MFT Encoder: '{}' (async={})", chosen_name, chosen_is_async);

            // Only unlock async if chosen MFT is actually async
            if chosen_is_async {
                if let Ok(attrs) = transform.GetAttributes() {
                    let _ = attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1);
                }
            }

            if let Ok(attrs) = transform.GetAttributes() {
                let low_latency = windows::core::GUID::from_u128(0x9c27891a_ed7a_40e1_88e8_292c34c343da);
                let real_time = windows::core::GUID::from_u128(0xc03eb300_0985_4228_b384_5f532f83ee18);
                let gop_size = windows::core::GUID::from_u128(0x95f34a07_8bd9_47e8_a581_9389ac525792);
                let _ = attrs.SetUINT32(&low_latency, 1);
                let _ = attrs.SetUINT32(&real_time, 1);
                let _ = attrs.SetUINT32(&gop_size, 30);
            }

            // Configure Output Media Type (H.264)
            let out_media: IMFMediaType = self.mf.create_media_type()?;
            out_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            out_media.SetUINT32(&MF_MT_AVG_BITRATE, self.bitrate)?;

            let frame_rate = ((self.fps as u64) << 32) | 1u64;
            out_media.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;

            let frame_size = ((self.width as u64) << 32) | (self.height as u64);
            out_media.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            out_media.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?; // 2 = MFVideoInterlace_Progressive

            transform.SetOutputType(0, &out_media, 0)?;

            // Configure Input Media Type (NV12)
            let in_media: IMFMediaType = self.mf.create_media_type()?;
            in_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            in_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            in_media.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            in_media.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;
            in_media.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;
            let _ = in_media.SetUINT32(&MF_MT_DEFAULT_STRIDE, self.width);

            transform.SetInputType(0, &in_media, 0)?;

            let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
            let _ = transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);

            if let Ok(stream_info) = transform.GetOutputStreamInfo(0) {
                self.out_buf_size = stream_info.cbSize.max(65536);
            }

            self.transform = Some(transform);
            info!("H.264 MFT Encoder initialized for {}x{} @ {} FPS ({} kbps)", self.width, self.height, self.fps, self.bitrate / 1000);
            Ok(())
        }
    }
}

impl VideoEncoder for H264MfEncoder {
    fn codec_type(&self) -> CodecType {
        CodecType::H264
    }

    fn encode(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        _force_keyframe: bool,
    ) -> Result<EncodedFrame> {
        let transform = self.transform.as_ref().ok_or_else(|| anyhow!("H264 encoder transform uninitialized"))?;

        if width != self.width || height != self.height {
            return Err(anyhow!("Resolution changed during H264 session: expected {}x{}, got {}x{}", self.width, self.height, width, height));
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
            if _force_keyframe {
                let _ = sample.SetUINT32(&windows::Win32::Media::MediaFoundation::MFSampleExtension_CleanPoint, 1);
            }

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
                        codec: CodecType::H264,
                        is_keyframe: false,
                        data: Vec::new(),
                        orig_width: width,
                        orig_height: height,
                    });
                }
                return Err(anyhow!("MFT ProcessOutput failed: {:?}", e));
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
                codec: CodecType::H264,
                is_keyframe,
                data: out_bytes,
                orig_width: width,
                orig_height: height,
            })
        }
    }
}

/// Windows Media Foundation Hardware H.264 Video Decoder
pub struct H264MfDecoder {
    width: u32,
    height: u32,
    transform: Option<IMFTransform>,
    mf: Option<Arc<MfPlatSymbols>>,
    out_buf_size: u32,
    frame_index: u64,
    last_frame: Option<Vec<u8>>,
}

unsafe impl Send for H264MfDecoder {}

impl H264MfDecoder {
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
                guidSubtype: MFVideoFormat_H264,
            };
            let mut activate_ptrs = std::ptr::null_mut();
            let mut count = 0u32;
            mf.mft_enum_ex(
                MFT_CATEGORY_VIDEO_DECODER,
                MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
                Some(&in_type),
                None,
                &mut activate_ptrs,
                &mut count,
            )?;
            if count == 0 || activate_ptrs.is_null() {
                return Err(anyhow!("No H.264 MFT video decoder found on this system"));
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

            let transform = chosen_transform.ok_or_else(|| anyhow!("No H.264 MFT video decoder could be activated"))?;

            if let Ok(attrs) = transform.GetAttributes() {
                let _ = attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1);
                let low_latency = windows::core::GUID::from_u128(0x9c27891a_ed7a_40e1_88e8_292c34c343da);
                let _ = attrs.SetUINT32(&low_latency, 1);
            }

            let frame_size = ((width as u64) << 32) | (height as u64);
            let frame_rate = (60u64 << 32) | 1u64;

            let in_media: IMFMediaType = mf.create_media_type()?;
            in_media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            in_media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
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
            info!("H.264 Media Foundation Decoder initialized for {}x{}", width, height);
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
                        // ContiguousCopyTo outputs unpadded rows (stride = expected_width).
                        // Determine the exact dimensions of the contiguous buffer.
                        let (contig_stride, contig_h) = if nv12.len() == (expected_width * expected_height * 3 / 2) as usize {
                            (expected_width as usize, expected_height as usize)
                        } else if dec_w as usize > expected_width as usize && nv12.len() == (dec_w * dec_h * 3 / 2) as usize {
                            (dec_w as usize, dec_h as usize)
                        } else if dec_h as usize > expected_height as usize && nv12.len() == (expected_width * dec_h * 3 / 2) as usize {
                            (expected_width as usize, dec_h as usize)
                        } else {
                            (expected_width as usize, expected_height as usize)
                        };

                        nv12_to_bgra_strided(
                            nv12,
                            expected_width as usize,
                            expected_height as usize,
                            contig_stride,
                            contig_h,
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

impl Default for H264MfDecoder {
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

impl VideoDecoder for H264MfDecoder {
    fn codec_type(&self) -> CodecType {
        CodecType::H264
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

            Err(anyhow!("H.264 decoder needs more input"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nv12_conversion_1080p_chroma_alignment() {
        let width = 1920usize;
        let height = 1080usize; // 1080 % 16 != 0 -> mb_h is 1088
        let mut bgra = vec![255u8; width * height * 4]; // Pure white background

        // Draw a green box at y in [200..250], x in [200..300]
        for y in 200..250 {
            for x in 200..300 {
                let px = (y * width + x) * 4;
                bgra[px] = 0;       // B
                bgra[px + 1] = 255; // G
                bgra[px + 2] = 0;   // R
                bgra[px + 3] = 255; // A
            }
        }

        // Draw a dark/black text-like line at y = 100, x in [100..500]
        for y in 100..105 {
            for x in 100..500 {
                let px = (y * width + x) * 4;
                bgra[px] = 0;
                bgra[px + 1] = 0;
                bgra[px + 2] = 0;
                bgra[px + 3] = 255;
            }
        }

        let nv12_len = width * height * 3 / 2;
        let mut nv12 = vec![0u8; nv12_len];
        bgra_to_nv12(&bgra, width, height, &mut nv12);

        let mut decoded_bgra = vec![0u8; width * height * 4];
        // Test decoding with contiguous stride and coded_h = 1088 (MFT decoder output type)
        nv12_to_bgra_strided(&nv12, width, height, width, 1088, &mut decoded_bgra);

        // Verify that 16 pixels below the black line (y = 116..121), there is NO green ghosting (it should be white)
        for y in 116..121 {
            for x in 100..500 {
                let px = (y * width + x) * 4;
                let b = decoded_bgra[px];
                let g = decoded_bgra[px + 1];
                let r = decoded_bgra[px + 2];
                // In white background, r, g, b are all close to 255 (BT.601 quantization gives ~235-255)
                assert!(r > 200, "Expected white background, got R={}", r);
                assert!(g > 200, "Expected white background, got G={}", g);
                assert!(b > 200, "Expected white background, got B={}", b);
                assert!((r as i32 - g as i32).abs() < 25, "Ghosting detected at y={}: R={} G={} B={}", y, r, g, b);
            }
        }

        // Verify that inside the green box (y = 220, x = 250), it is green
        let px_green = (220 * width + 250) * 4;
        let b = decoded_bgra[px_green];
        let g = decoded_bgra[px_green + 1];
        let r = decoded_bgra[px_green + 2];
        assert!(g > 180, "Green box should be green, got G={}", g);
        assert!(r < 60, "Green box should have low red, got R={}", r);
        assert!(b < 60, "Green box should have low blue, got B={}", b);

        // Verify that 16 pixels below the green box (y = 266), it is WHITE, NOT GREEN!
        let px_below_green = (266 * width + 250) * 4;
        let b_bg = decoded_bgra[px_below_green];
        let g_bg = decoded_bgra[px_below_green + 1];
        let r_bg = decoded_bgra[px_below_green + 2];
        assert!(r_bg > 200, "Chroma ghosting below green box! Got R={}", r_bg);
        assert!(g_bg > 200, "Chroma ghosting below green box! Got G={}", g_bg);
        assert!(b_bg > 200, "Chroma ghosting below green box! Got B={}", b_bg);
    }
}



