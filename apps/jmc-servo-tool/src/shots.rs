//! Test only: draws frames off the screen with wgpu and saves them as PNG files. With this the
//! window can be looked at without opening it on the screen.

use std::path::Path;

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};

/// Gamma-space colour, as egui expects for its output.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct Shooter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    /// Screen size in pixels.
    size: [u32; 2],
    pixels_per_point: f32,
    /// RGBA pixels of the last frame.
    pixels: Vec<u8>,
}

impl Shooter {
    /// `None` when there is no GPU adapter (for example on a build server).
    pub fn new(size: [u32; 2], pixels_per_point: f32) -> Option<Self> {
        let instance = wgpu::Instance::default();
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        let renderer = egui_wgpu::Renderer::new(&device, FORMAT, egui_wgpu::RendererOptions::default());
        Some(Self { device, queue, renderer, size, pixels_per_point, pixels: Vec::new() })
    }

    /// The screen size in points.
    pub fn screen(&self) -> egui::Vec2 {
        egui::vec2(self.size[0] as f32, self.size[1] as f32) / self.pixels_per_point
    }

    /// Runs and draws one frame. Every frame of the context must go through here, because egui
    /// sends the font texture once and then only the changes.
    pub fn frame(&mut self, ctx: &egui::Context, events: Vec<egui::Event>, run: impl FnMut(&mut egui::Ui)) {
        let [w, h] = self.size;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.screen())),
            events,
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(self.pixels_per_point);
        let mut out = ctx.run_ui(input, run);
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: self.size, pixels_per_point: out.pixels_per_point };
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shot"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let extra = self.renderer.update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &jobs, &screen);
        }
        // Rows of a texture copy are padded to 256 bytes.
        let row = (w * 4).next_multiple_of(256);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shot"),
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            extent,
        );
        self.queue.submit(extra.into_iter().chain([encoder.finish()]));
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).expect("GPU wait");
        {
            let data = buffer.slice(..).get_mapped_range().expect("mapped buffer");
            self.pixels.clear();
            for y in 0..h as usize {
                self.pixels.extend_from_slice(&data[y * row as usize..][..w as usize * 4]);
            }
        }
        buffer.unmap();
        for id in &out.textures_delta.free {
            self.renderer.free_texture(id);
        }
        // Handled above; epaint checks that it was.
        out.textures_delta.clear();
    }

    /// Saves the last frame, or the part of it in `crop` (points), as a PNG file.
    pub fn save(&self, path: &Path, crop: Option<egui::Rect>) -> std::io::Result<()> {
        let [w, h] = self.size;
        let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w as f32, h as f32));
        let r = crop.map_or(full, |c| (c * self.pixels_per_point).round().intersect(full));
        let (x0, y0) = (r.min.x as usize, r.min.y as usize);
        let (cw, ch) = (r.width() as usize, r.height() as usize);
        let mut rgba = Vec::with_capacity(cw * ch * 4);
        for y in y0..y0 + ch {
            let start = (y * w as usize + x0) * 4;
            rgba.extend_from_slice(&self.pixels[start..start + cw * 4]);
        }
        std::fs::write(path, png(cw as u32, ch as u32, &rgba))
    }
}

/// Runs a future that is ready at once, or nearly: wgpu's on native.
fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

/// An RGBA PNG file with stored (not compressed) deflate blocks: big, but simple.
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for line in rgba.chunks(w as usize * 4) {
        raw.push(0); // no filter
        raw.extend_from_slice(line);
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(0xFFFF).peekable();
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&w.to_be_bytes());
    header.extend_from_slice(&h.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[test]
fn png_checksums() {
    // Known values: CRC-32 and Adler-32 of "123456789".
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(adler32(b"123456789"), 0x091E_01DE);
}
