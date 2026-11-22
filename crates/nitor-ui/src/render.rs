//! Вибір рушія малювання вікна — спільний для програми й інсталятора.

/// Рендер вікна. Запасний OpenGL у wgpu на старих драйверах (Intel без DX12) малює все, крім літер, тому
/// wgpu працює лише через DirectX 12 / Vulkan / Metal (зокрема програмний WARP, якщо відеокарти немає),
/// а якщо відеокарта є лише з OpenGL або DX12 / Vulkan немає зовсім — glow. `NITOR_RENDERER=glow|wgpu`.
pub fn set_renderer(o: &mut eframe::NativeOptions) {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    let all = wgpu::Backends::PRIMARY | wgpu::Backends::GL;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor { backends: all, ..Default::default() });
    let gpus: Vec<_> = instance.enumerate_adapters(all).into_iter().map(|a| a.get_info()).collect();
    for i in &gpus {
        log::info!("Відеоадаптер: {} ({:?}, {:?})", i.name, i.backend, i.device_type);
    }
    let hw = |i: &&wgpu::AdapterInfo| i.device_type != wgpu::DeviceType::Cpu;
    let primary = |i: &&wgpu::AdapterInfo| i.backend != wgpu::Backend::Gl;
    let primary_hw = gpus.iter().filter(hw).any(|i| primary(&i));
    let primary_any = gpus.iter().any(|i| primary(&i));
    let gl_hw = gpus.iter().filter(hw).any(|i| i.backend == wgpu::Backend::Gl);
    o.renderer = match std::env::var("NITOR_RENDERER").as_deref() {
        Ok("glow") => eframe::Renderer::Glow,
        Ok("wgpu") => eframe::Renderer::Wgpu,
        _ if !primary_hw && (gl_hw || !primary_any) => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let mut setup = WgpuSetupCreateNew::default();
    setup.instance_descriptor.backends = wgpu::Backends::PRIMARY;
    o.wgpu_options = WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() };
    log::info!("Рендер вікна: {:?}", o.renderer);
}
