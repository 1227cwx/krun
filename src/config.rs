use crate::win::wide;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};

const CONFIG_FILE_NAME: &str = "launcher.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub window: WindowConfig,
    pub hotkey: HotkeyConfig,
    pub double_click_launch: bool,
    pub active_category: String,
    pub categories: Vec<Category>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WindowConfig {
    pub width: i32,
    pub height: i32,
    pub monitor: String,
    pub x: i32,
    pub y: i32,
    pub centered: bool,
    pub movable: bool,
    pub resizable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HotkeyConfig {
    pub modifiers: u32,
    pub key: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub items: Vec<LaunchItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct LaunchItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub arguments: String,
    pub working_directory: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 3,
            window: WindowConfig::default(),
            hotkey: HotkeyConfig::default(),
            double_click_launch: true,
            active_category: "frequent".into(),
            categories: vec![Category::new("frequent", "常用")],
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 840,
            height: 520,
            monitor: String::new(),
            x: 80,
            y: 80,
            centered: true,
            movable: false,
            resizable: true,
        }
    }
}

impl WindowConfig {
    pub fn normalize(&mut self) {
        self.width = self.width.clamp(620, 1600);
        self.height = self.height.clamp(400, 1200);
        if self.centered {
            self.movable = false;
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifiers: 0x0001,
            key: b'Q' as u32,
        }
    }
}

impl Default for Category {
    fn default() -> Self {
        Self::new("category", "分类")
    }
}

impl Category {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            items: Vec::new(),
        }
    }
}

impl Config {
    pub fn normalize(&mut self) {
        self.version = 3;
        self.window.normalize();
        if self.categories.is_empty() {
            self.categories.push(Category::new("frequent", "常用"));
        }
        if !self
            .categories
            .iter()
            .any(|category| category.id == self.active_category)
        {
            self.active_category = self.categories[0].id.clone();
        }
    }

    pub fn active_category_index(&self) -> usize {
        self.categories
            .iter()
            .position(|category| category.id == self.active_category)
            .unwrap_or(0)
    }

    pub fn unique_category_id(&self, name: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut attempt = 0u64;
        loop {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            name.hash(&mut hasher);
            attempt.hash(&mut hasher);
            let candidate = format!("category-{:016x}", hasher.finish());
            if !self
                .categories
                .iter()
                .any(|category| category.id == candidate)
            {
                return candidate;
            }
            attempt += 1;
        }
    }
}

pub fn executable_directory() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "无法确定程序所在目录"))
}

pub fn config_path() -> io::Result<PathBuf> {
    Ok(executable_directory()?.join(CONFIG_FILE_NAME))
}

pub fn load(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let bytes = fs::read(path).map_err(|error| format!("读取配置失败：{error}"))?;
    let mut config: Config =
        serde_json::from_slice(&bytes).map_err(|error| format!("配置格式无效：{error}"))?;
    config.normalize();
    Ok(config)
}

/// Serialises a configuration into the on-disk JSON representation.
pub fn encode(config: &Config) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(config).map_err(|error| format!("序列化配置失败：{error}"))
}

/// Writes pre-serialised bytes next to the executable, replacing the previous
/// file atomically so a crash can never leave a half-written configuration.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "配置文件路径没有父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;

    let temporary = parent.join(format!("{CONFIG_FILE_NAME}.tmp"));
    let mut file =
        fs::File::create(&temporary).map_err(|error| format!("创建临时配置失败：{error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("写入临时配置失败：{error}"))?;
    drop(file);

    let source = wide(temporary.as_os_str());
    let destination = wide(path.as_os_str());
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(format!("保存配置失败：{}", io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_one_centered_category() {
        let config = Config::default();
        assert_eq!(config.categories.len(), 1);
        assert_eq!(config.categories[0].name, "常用");
        assert!(config.window.centered);
        assert!(!config.window.movable);
        assert!(config.double_click_launch);
    }

    #[test]
    fn old_json_is_upgraded_without_losing_categories() {
        let json = r#"{
            "version": 1,
            "window": { "width": 880, "height": 560 },
            "hotkey": { "modifiers": 1, "key": 81 },
            "active_category": "tools",
            "categories": [
                { "id": "frequent", "name": "常用", "items": [] },
                { "id": "tools", "name": "工具", "items": [] }
            ]
        }"#;
        let mut restored: Config = serde_json::from_str(json).unwrap();
        restored.normalize();
        assert_eq!(restored.categories.len(), 2);
        assert_eq!(restored.active_category, "tools");
        assert!(restored.window.centered);
        assert!(restored.double_click_launch);
    }

    #[test]
    fn centered_always_disables_moving() {
        let mut window = WindowConfig {
            centered: true,
            movable: true,
            ..WindowConfig::default()
        };
        window.normalize();
        assert!(!window.movable);
    }

    #[test]
    fn category_ids_are_unique() {
        let config = Config::default();
        let first = config.unique_category_id("测试");
        let mut with_first = config;
        with_first.categories.push(Category::new(&first, "测试"));
        assert_ne!(first, with_first.unique_category_id("测试"));
    }

    #[test]
    fn json_round_trip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, config);
    }

    #[test]
    fn encoded_bytes_are_replaced_atomically() {
        let directory = std::env::temp_dir().join(format!(
            "krun-config-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(CONFIG_FILE_NAME);

        let mut config = Config::default();
        config.categories.push(Category::new("tools", "工具"));
        let bytes = encode(&config).unwrap();
        write_atomic(&path, &bytes).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);

        // A second write replaces the first one instead of appending to it.
        config.categories[0].items.push(LaunchItem {
            id: "one".into(),
            name: "项目".into(),
            path: "%WINDIR%\\explorer.exe".into(),
            ..LaunchItem::default()
        });
        let updated = encode(&config).unwrap();
        write_atomic(&path, &updated).unwrap();
        assert_eq!(load(&path).unwrap(), config);

        let leftovers = fs::read_dir(&directory).unwrap().count();
        assert_eq!(leftovers, 1, "the temporary file should be gone");
        fs::remove_dir_all(&directory).ok();
    }

    fn generated_config(categories: usize, items: usize) -> Config {
        let mut config = Config::default();
        config.categories = (0..categories)
            .map(|category| {
                let mut value =
                    Category::new(format!("category-{category}"), format!("分类 {category}"));
                value.items = (0..items)
                    .map(|item| LaunchItem {
                        id: format!("item-{category}-{item}"),
                        name: format!("示例项目 {item}"),
                        path: format!("%WINDIR%\\System32\\example-{category}-{item}.exe"),
                        ..LaunchItem::default()
                    })
                    .collect();
                value
            })
            .collect();
        config.active_category = config.categories[0].id.clone();
        config
    }

    /// Measures load/encode/write for realistically large configurations.
    /// Run explicitly with:
    /// `cargo test --release perf -- --ignored --nocapture`
    #[test]
    #[ignore = "timing harness, not an assertion"]
    fn perf_config_sizes() {
        let directory = std::env::temp_dir().join("krun-config-perf");
        fs::create_dir_all(&directory).unwrap();
        for (categories, items) in [(1usize, 1_000usize), (1, 10_000), (1, 50_000)] {
            let config = generated_config(categories, items);
            let total = categories * items;
            let started = std::time::Instant::now();
            let bytes = encode(&config).unwrap();
            let encoded = started.elapsed();
            let path = directory.join("launcher.json");
            let started = std::time::Instant::now();
            write_atomic(&path, &bytes).unwrap();
            let written = started.elapsed();
            let started = std::time::Instant::now();
            let restored = load(&path).unwrap();
            let loaded = started.elapsed();
            assert_eq!(restored, config);
            println!(
                "{total:>6} 项 / {:>7} 字节：encode {:>8.2?}  写入 {:>8.2?}  读取 {:>8.2?}",
                bytes.len(),
                encoded,
                written,
                loaded
            );
        }
        fs::remove_dir_all(&directory).ok();
    }
}
