use crate::config::Config;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Posted to the window when a background write failed.
pub const SAVE_ERROR_MESSAGE: u32 = WM_APP + 6;

struct Job {
    path: PathBuf,
    config: Config,
    completed: Option<SyncSender<()>>,
}

/// Serialises and writes configuration on a worker thread so interaction never
/// blocks on JSON encoding or disk I/O. The atomic replacement itself is
/// unchanged and still lives in [`crate::config::write_atomic`].
///
/// Handing over an owned [`Config`] costs one clone on the caller's thread but
/// keeps serialisation — the part that scales with the number of items — off
/// the UI thread.
pub struct ConfigWriter {
    sender: Option<Sender<Job>>,
    handle: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}

impl ConfigWriter {
    pub fn new(hwnd: HWND) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let error = Arc::new(Mutex::new(None));
        let worker_error = Arc::clone(&error);
        let hwnd_value = hwnd as usize;
        let handle = thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                let result = crate::config::encode(&job.config)
                    .and_then(|bytes| crate::config::write_atomic(&job.path, &bytes));
                if let Err(message) = result {
                    if let Ok(mut slot) = worker_error.lock() {
                        *slot = Some(message);
                    }
                    unsafe { PostMessageW(hwnd_value as HWND, SAVE_ERROR_MESSAGE, 0, 0) };
                }
                if let Some(completed) = job.completed {
                    let _ = completed.send(());
                }
            }
        });
        Self {
            sender: Some(sender),
            handle: Some(handle),
            error,
        }
    }

    pub fn write(&self, path: PathBuf, config: Config) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Job {
                path,
                config,
                completed: None,
            });
        }
    }

    /// Writes one configuration and waits for the worker to finish it without
    /// shutting the worker down. This is used for Windows session shutdown,
    /// which the user can still cancel after WM_QUERYENDSESSION.
    pub fn write_and_wait(&self, path: PathBuf, config: Config) {
        let Some(sender) = &self.sender else {
            return;
        };
        let (completed, receiver) = mpsc::sync_channel(0);
        if sender
            .send(Job {
                path,
                config,
                completed: Some(completed),
            })
            .is_ok()
        {
            let _ = receiver.recv();
        }
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut slot| slot.take())
    }

    /// Blocks until every queued write has finished. Safe to call twice.
    pub fn finish(&mut self) {
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ConfigWriter {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LaunchItem;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("krun-writer-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn config_named(name: &str) -> Config {
        let mut config = Config::default();
        config.categories[0].items.push(LaunchItem {
            id: name.into(),
            name: name.into(),
            path: format!("%WINDIR%\\{name}.exe"),
            ..LaunchItem::default()
        });
        config
    }

    #[test]
    fn queued_writes_are_flushed_before_finish_returns() {
        let directory = temporary_directory();
        let path = directory.join("launcher.json");
        let mut writer = ConfigWriter::new(std::ptr::null_mut());
        writer.write(path.clone(), config_named("alpha"));
        writer.finish();
        let restored = crate::config::load(&path).unwrap();
        assert_eq!(restored.categories[0].items[0].name, "alpha");
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn synchronous_write_keeps_worker_available() {
        let directory = temporary_directory();
        let path = directory.join("launcher.json");
        let mut writer = ConfigWriter::new(std::ptr::null_mut());
        writer.write_and_wait(path.clone(), config_named("first"));
        let restored = crate::config::load(&path).unwrap();
        assert_eq!(restored.categories[0].items[0].name, "first");

        writer.write(path.clone(), config_named("second"));
        writer.finish();
        let restored = crate::config::load(&path).unwrap();
        assert_eq!(restored.categories[0].items[0].name, "second");
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn the_last_queued_write_wins() {
        let directory = temporary_directory();
        let path = directory.join("launcher.json");
        let mut writer = ConfigWriter::new(std::ptr::null_mut());
        for round in 0..8 {
            writer.write(path.clone(), config_named(&format!("round-{round}")));
        }
        writer.finish();
        let restored = crate::config::load(&path).unwrap();
        assert_eq!(restored.categories[0].items[0].name, "round-7");
        fs::remove_dir_all(&directory).ok();
    }

    /// Encoding now happens on the worker; this confirms the UI-thread share
    /// (cloning the config) is much cheaper than encoding it inline.
    ///
    /// Run with: `cargo test --release perf_encode -- --ignored --nocapture`
    #[test]
    #[ignore = "timing harness, not an assertion"]
    fn perf_clone_versus_encode() {
        for items in [1_000usize, 10_000, 50_000] {
            let mut config = Config::default();
            config.categories[0].items = (0..items)
                .map(|item| LaunchItem {
                    id: format!("item-{item}"),
                    name: format!("示例项目 {item}"),
                    path: format!("%WINDIR%\\System32\\example-{item}.exe"),
                    ..LaunchItem::default()
                })
                .collect();

            let started = std::time::Instant::now();
            for _ in 0..20 {
                std::hint::black_box(config.clone());
            }
            let clone = started.elapsed() / 20;

            let started = std::time::Instant::now();
            for _ in 0..20 {
                std::hint::black_box(crate::config::encode(&config).unwrap());
            }
            let encode = started.elapsed() / 20;

            println!(
                "{items:>6} 项：界面线程 clone {clone:>9.2?} → 工作线程 encode {encode:>9.2?}（界面线程 {:.0}× 更省）",
                encode.as_secs_f64() / clone.as_secs_f64().max(f64::MIN_POSITIVE)
            );
        }
    }
}
