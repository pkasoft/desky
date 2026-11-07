use anyhow::Result;
use tracing::info;
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1,
};

pub fn diagnose_dxgi() -> Result<()> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapter_index = 0;

        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            let desc = adapter.GetDesc1()?;
            let gpu_name = String::from_utf16_lossy(
                &desc.Description[..desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len())]
            );
            info!("Adapter [{}]: {} (Dedicated VRAM: {} MB, Flags: {:?})", 
                adapter_index, gpu_name, desc.DedicatedVideoMemory / (1024 * 1024), desc.Flags);

            let mut output_index = 0;
            while let Ok(output) = adapter.EnumOutputs(output_index) {
                let out_desc = output.GetDesc()?;
                let out_name = String::from_utf16_lossy(
                    &out_desc.DeviceName[..out_desc.DeviceName.iter().position(|&c| c == 0).unwrap_or(out_desc.DeviceName.len())]
                );
                let w = (out_desc.DesktopCoordinates.right - out_desc.DesktopCoordinates.left).unsigned_abs();
                let h = (out_desc.DesktopCoordinates.bottom - out_desc.DesktopCoordinates.top).unsigned_abs();
                info!("  -> Output [{}]: {} ({}x{}, AttachedToDesktop: {:?})", 
                    output_index, out_name, w, h, out_desc.AttachedToDesktop);

                let mut device: Option<ID3D11Device> = None;
                let mut context: Option<ID3D11DeviceContext> = None;
                let mut feature_level = Default::default();
                let hr = D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut feature_level),
                    Some(&mut context),
                );

                if let Ok(()) = hr {
                    let output1: Result<IDXGIOutput1, _> = output.cast();
                    if let Ok(output1) = output1 {
                        let dev = device.unwrap();
                        match output1.DuplicateOutput(&dev) {
                            Ok(dupl) => {
                                info!("     [SUCCESS] DuplicateOutput acquired successfully!");
                                drop(dupl);
                            }
                            Err(e) => {
                                info!("     [FAIL] DuplicateOutput returned: {} ({:#x})", e.message(), e.code().0);
                            }
                        }
                    }
                }

                output_index += 1;
            }

            adapter_index += 1;
        }
    }
    Ok(())
}
