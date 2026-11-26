//! Draws the diagnostics HUD into the swap-chain buffer, after the title's
//! frame has been copied. The title's render targets are left intact.
use crate::host::{Host, HostList};
use std::collections::VecDeque;
use std::os::windows::process::CommandExt;
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*, Gdi::*};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F9};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 428;
const PITCH: u32 = WIDTH * 4;
const BYTES: usize = (PITCH * HEIGHT) as usize;
const OFFSET: u32 = 12;

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("VICTORY_HUD").as_deref() == Ok("1"))
}

#[derive(Default)]
pub(crate) struct Hud {
    pixels: Option<(ID3D12Resource, u32)>,
    cpu: Option<[u64; 3]>,
    console: VecDeque<String>,
    hidden: bool,
    toggle_held: bool,
}

impl Hud {
    pub(crate) fn update(&mut self, host: &Host, timing: [&str; 2], size: (u32, u32)) {
        if !enabled() {
            return;
        }
        let (resident, private, used, total) = memory();
        let vram = host.video_memory().map_or_else(
            || "VRAM n/d".to_owned(),
            |[budget, local, _, shared]| {
                format!(
                    "VRAM DXGI {}/{} MiB   Compartilhada {} MiB",
                    local >> 20,
                    budget >> 20,
                    shared >> 20
                )
            },
        );
        let ram = format!("RAM {used}/{total} MiB   Jogo {resident} residente / {private} privada");
        let gpu = gpu_sample();
        let cpu = cpu_sample(&mut self.cpu);
        tracing::info!(target: "victory_gpu::hud", timing = timing[0], stages = timing[1], %cpu, %gpu, %ram, %vram, "HUD sample");
        if self.console.len() == 7 {
            self.console.pop_front();
        }
        self.console
            .push_back(format!("{:>6}  {}", crate::queue::presented(), timing[0]));
        let mut lines = vec![
            "GALOCEGO   |   DESEMPENHO                     F9 mostrar/ocultar",
            timing[0],
            timing[1],
            &cpu,
            &gpu,
            &vram,
            &ram,
            "WASD mover   IJKL camera   Enter A   Backspace B   Esc menu",
            "",
            "CONSOLE   |   ULTIMAS AMOSTRAS",
            " Quadro  Medicao do Victory",
        ];
        lines.extend(self.console.iter().rev().map(String::as_str));
        let requested = std::env::var("VICTORY_HUD_SCALE")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(2)
            .clamp(1, 3);
        let scale = requested
            .min(size.0.saturating_sub(OFFSET) / WIDTH)
            .min(size.1.saturating_sub(OFFSET) / HEIGHT);
        if scale == 0 {
            self.pixels = None;
            return;
        }
        if let Some((buffer, data)) = host.upload_buffer(BYTES as u64 * u64::from(scale * scale)) {
            if paint(data, &lines, scale as usize) {
                self.pixels = Some((buffer, scale));
            }
        }
    }

    pub(crate) fn copy(&mut self, copy: &HostList, back: &ID3D12Resource, hwnd: HWND) -> bool {
        if !enabled() {
            return false;
        }
        // SAFETY: these calls only query the foreground window and F9 state.
        let held =
            unsafe { GetForegroundWindow() == hwnd && GetAsyncKeyState(i32::from(VK_F9)) < 0 };
        if held && !self.toggle_held {
            self.hidden = !self.hidden;
        }
        self.toggle_held = held;
        if self.hidden {
            return false;
        }
        let Some((upload, scale)) = &self.pixels else {
            return false;
        };
        // SAFETY: the presenter has put the back buffer in COPY_DEST.
        // The upload is immutable and held by the command list until its fence.
        unsafe {
            let desc = back.GetDesc();
            if desc.Width < u64::from(WIDTH * scale + OFFSET)
                || desc.Height < HEIGHT * scale + OFFSET
                || !matches!(
                    desc.Format,
                    DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM
                )
            {
                return false;
            }
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(upload),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: desc.Format,
                            Width: WIDTH * scale,
                            Height: HEIGHT * scale,
                            Depth: 1,
                            RowPitch: PITCH * scale,
                        },
                    },
                },
            };
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(back),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            copy.keep(upload);
            copy.list.CopyTextureRegion(
                &dst,
                desc.Width as u32 - WIDTH * scale - OFFSET,
                OFFSET,
                0,
                &src,
                None,
            );
        }
        true
    }
}

fn cpu_sample(previous: &mut Option<[u64; 3]>) -> String {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessTimes, GetSystemTimes,
    };
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    let [
        mut created,
        mut exited,
        mut pk,
        mut pu,
        mut idle,
        mut sk,
        mut su,
    ] = [FILETIME::default(); 7];
    // SAFETY: both calls fill the supplied FILETIME values.
    unsafe {
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut pk,
            &mut pu,
        ) == 0
            || GetSystemTimes(&mut idle, &mut sk, &mut su) == 0
        {
            return "CPU: n/d".into();
        }
    }
    let now = [ticks(pk) + ticks(pu), ticks(idle), ticks(sk) + ticks(su)];
    let Some(old) = previous.replace(now) else {
        return "CPU: aguardando amostra".into();
    };
    let [game, idle, total] = std::array::from_fn(|i| now[i].saturating_sub(old[i]) as f64);
    let total = total.max(1.0);
    format!(
        "CPU jogo {:.0}%   CPU total {:.0}%",
        100.0 * game / total,
        100.0 * (total - idle) / total
    )
}

fn paint(out: *mut u8, lines: &[&str], scale: usize) -> bool {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: WIDTH as i32,
            biHeight: -(HEIGHT as i32),
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: private DC and DIB. Both are released before returning. Pixels
    // use neutral RGB so the same data works with RGBA and BGRA back buffers.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return false;
        }
        let mut bits = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) => bitmap,
            Err(_) => {
                let _ = DeleteDC(dc);
                return false;
            }
        };
        let old = SelectObject(dc, bitmap.into());
        std::ptr::write_bytes(bits.cast::<u8>(), 24, BYTES);
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00f4f4f4));
        let font = SelectObject(dc, GetStockObject(ANSI_FIXED_FONT));
        for (index, line) in lines.iter().enumerate() {
            let text: Vec<u16> = line.encode_utf16().collect();
            if !text.is_empty() {
                let _ = TextOutW(dc, 12, 8 + index as i32 * 22, &text);
            }
        }
        let _ = GdiFlush();
        let data = std::slice::from_raw_parts_mut(bits.cast::<u8>(), BYTES);
        for pixel in data.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        let pitch = PITCH as usize * scale;
        let output = std::slice::from_raw_parts_mut(out, BYTES * scale * scale);
        for (source, dest) in data
            .chunks_exact(PITCH as usize)
            .zip(output.chunks_exact_mut(pitch * scale))
        {
            let (first, rest) = dest.split_at_mut(pitch);
            for (pixel, enlarged) in source
                .chunks_exact(4)
                .zip(first.chunks_exact_mut(4 * scale))
            {
                for cell in enlarged.chunks_exact_mut(4) {
                    cell.copy_from_slice(pixel);
                }
            }
            for row in rest.chunks_exact_mut(pitch) {
                row.copy_from_slice(first);
            }
        }
        SelectObject(dc, font);
        SelectObject(dc, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
    }
    true
}

fn memory() -> (usize, usize, u64, u64) {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut p = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: both calls fill fixed-size local structures.
    unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut p).cast(), p.cb);
        GlobalMemoryStatusEx(&mut m);
    }
    (
        p.WorkingSetSize >> 20,
        p.PrivateUsage >> 20,
        m.ullTotalPhys.saturating_sub(m.ullAvailPhys) >> 20,
        m.ullTotalPhys >> 20,
    )
}

fn gpu_sample() -> String {
    static SAMPLE: OnceLock<Mutex<String>> = OnceLock::new();
    let sample = SAMPLE.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("victory-hud-gpu".into())
            .spawn(|| {
                let Some(sample) = SAMPLE.get() else {
                    // Wait for get_or_init to publish the sample cell.
                    let sample = SAMPLE.wait();
                    return poll_gpu(sample);
                };
                poll_gpu(sample);
            });
        Mutex::new("GPU: aguardando amostra".into())
    });
    sample.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn poll_gpu(sample: &Mutex<String>) {
    loop {
        let output = std::process::Command::new("nvidia-smi.exe")
            .creation_flags(0x0800_0000)
            .args(["--query-gpu=utilization.gpu,temperature.gpu,power.draw,clocks.current.graphics,clocks.current.memory", "--format=csv,noheader,nounits"])
            .output();
        let line = output
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok());
        let text = line
            .as_deref()
            .and_then(|line| {
                let mut v = line.lines().next()?.split(',').map(str::trim);
                Some(format!(
                    "GPU {}%   {} C   {} W   {} / {} MHz",
                    v.next()?,
                    v.next()?,
                    v.next()?,
                    v.next()?,
                    v.next()?
                ))
            })
            .unwrap_or_else(|| "GPU: nvidia-smi indisponivel".into());
        *sample.lock().unwrap_or_else(|e| e.into_inner()) = text;
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
