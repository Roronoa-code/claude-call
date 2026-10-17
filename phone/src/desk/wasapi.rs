use std::sync::{Arc, atomic::{AtomicBool, AtomicU32, Ordering}, mpsc::Sender};
use windows::Win32::{
    Foundation::{CloseHandle, WAIT_OBJECT_0},
    Media::Audio::*,
    System::{Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize}, Threading::{CreateEventW, WaitForSingleObject}},
};

pub struct Stream {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

enum Io {
    Out(Box<dyn FnMut(&mut [i16]) + Send>),
    In(Box<dyn FnMut(&[f32]) + Send>),
}

pub fn output(rate: u32, fill: impl FnMut(&mut [i16]) + Send + 'static) -> Option<(Stream, u32)> {
    open(rate, Io::Out(Box::new(fill))).map(|s| (s, rate))
}

pub fn input(rate: u32, opened: &AtomicU32, take: impl FnMut(&[f32]) + Send + 'static) -> Option<Stream> {
    opened.store(rate, Ordering::Relaxed);
    open(rate, Io::In(Box::new(take)))
}

fn open(rate: u32, mut io: Io) -> Option<Stream> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let s = stop.clone();
    let thread = std::thread::Builder::new()
        .name("wasapi".into())
        .spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            if let Err(e) = pump(rate, &mut io, &s, &tx) {
                crate::android::log(&format!("audio stream failed: {e}"));
                let _ = tx.send(false);
            }
            CoUninitialize();
        })
        .ok()?;
    if rx.recv().unwrap_or(false) {
        Some(Stream { stop, thread: Some(thread) })
    } else {
        let _ = thread.join();
        None
    }
}

// Communications category on the communications device lets Windows apply its echo cancelling.
unsafe fn pump(rate: u32, io: &mut Io, stop: &AtomicBool, ready: &Sender<bool>) -> windows::core::Result<()> {
    unsafe {
        let render = matches!(io, Io::Out(_));
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = devices.GetDefaultAudioEndpoint(if render { eRender } else { eCapture }, eCommunications)?;
        let client: IAudioClient2 = device.Activate(CLSCTX_ALL, None)?;
        let props = AudioClientProperties {
            cbSize: size_of::<AudioClientProperties>() as u32,
            bIsOffload: false.into(),
            eCategory: AudioCategory_Communications,
            Options: AUDCLNT_STREAMOPTIONS_NONE,
        };
        let _ = client.SetClientProperties(&props);
        let (bits, tag) = if render { (16u16, 1u16) } else { (32, 3) };
        let block = bits / 8;
        let format = WAVEFORMATEX {
            wFormatTag: tag,
            nChannels: 1,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * block as u32,
            nBlockAlign: block,
            wBitsPerSample: bits,
            cbSize: 0,
        };
        let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 400_000, 0, &format, None)?;
        let event = CreateEventW(None, false, false, None)?;
        client.SetEventHandle(event)?;
        let frames = client.GetBufferSize()?;
        let result = (|| {
            match io {
                Io::Out(fill) => {
                    let out: IAudioRenderClient = client.GetService()?;
                    client.Start()?;
                    let _ = ready.send(true);
                    while !stop.load(Ordering::Relaxed) {
                        if WaitForSingleObject(event, 200) != WAIT_OBJECT_0 {
                            continue;
                        }
                        let n = frames - client.GetCurrentPadding()?;
                        if n > 0 {
                            let p = out.GetBuffer(n)?;
                            fill(std::slice::from_raw_parts_mut(p.cast::<i16>(), n as usize));
                            out.ReleaseBuffer(n, 0)?;
                        }
                    }
                }
                Io::In(take) => {
                    let cap: IAudioCaptureClient = client.GetService()?;
                    client.Start()?;
                    let _ = ready.send(true);
                    let mut silence = Vec::new();
                    while !stop.load(Ordering::Relaxed) {
                        if WaitForSingleObject(event, 200) != WAIT_OBJECT_0 {
                            continue;
                        }
                        loop {
                            let (mut data, mut n, mut got) = (std::ptr::null_mut(), 0u32, 0u32);
                            cap.GetBuffer(&mut data, &mut n, &mut got, None, None)?;
                            if n == 0 {
                                break;
                            }
                            if got & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                                silence.resize(n as usize, 0.0);
                                take(&silence);
                            } else {
                                take(std::slice::from_raw_parts(data.cast::<f32>(), n as usize));
                            }
                            cap.ReleaseBuffer(n)?;
                        }
                    }
                }
            }
            Ok(())
        })();
        let _ = client.Stop();
        let _ = CloseHandle(event);
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn opens_speaker_and_mic_at_call_rates() {
        let played = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let p = played.clone();
        let Some((out, rate)) = super::output(24_000, move |b| {
            b.fill(0);
            p.store(true, std::sync::atomic::Ordering::Relaxed);
        }) else {
            return;
        };
        assert_eq!(rate, 24_000);
        let opened = std::sync::atomic::AtomicU32::new(0);
        let mic = super::input(16_000, &opened, |_| {});
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(played.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(opened.load(std::sync::atomic::Ordering::Relaxed), 16_000);
        drop((out, mic));
    }
}
