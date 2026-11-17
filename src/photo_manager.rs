use exif::{In, Reader as ExifReader, Tag};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

pub const MEDIA_DIR: &str = "trip_media";

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PhotoRecord {
    pub id: i64,
    pub stop_id: i64,
    pub file_path: String,
    pub taken_at: String,
    pub caption: String,
}

/// 4. 图片照片管理模块：负责 EXIF 解析、媒体文件复制存储与 GPU 缩略图缓存
pub struct PhotoManager {
    db_path: String,
    texture_cache: HashMap<String, egui::TextureHandle>,
}

impl PhotoManager {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        fs::create_dir_all(MEDIA_DIR).ok();
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS stop_photos (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 stop_id INTEGER NOT NULL,
                 file_path TEXT NOT NULL,
                 taken_at TEXT NOT NULL DEFAULT '',
                 caption TEXT NOT NULL DEFAULT ''
             );",
        )?;
        Ok(Self {
            db_path: db_path.to_string(),
            texture_cache: HashMap::new(),
        })
    }

    /// 导入本地照片到离线媒体目录，提取 EXIF 拍摄时间并存入 SQLite
    pub fn import_photo(
        &self,
        stop_id: i64,
        src_path: &Path,
    ) -> Result<PhotoRecord, Box<dyn std::error::Error>> {
        fs::create_dir_all(MEDIA_DIR)?;

        let mut taken_at = String::new();
        if let Ok(file) = File::open(src_path) {
            let mut bufreader = BufReader::new(&file);
            if let Ok(exif) = ExifReader::new().read_from_container(&mut bufreader) {
                if let Some(field) = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY) {
                    taken_at = field.display_value().with_unit(&exif).to_string();
                }
            }
        }

        let ext = src_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("jpg");
        let ts = chrono::Local::now().timestamp_millis();
        let dest_name = format!("stop_{}_{}.{}", stop_id, ts, ext);
        let dest_path = Path::new(MEDIA_DIR).join(&dest_name);
        fs::copy(src_path, &dest_path)?;

        let saved_path = dest_path.to_string_lossy().to_string();
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO stop_photos (stop_id, file_path, taken_at, caption) VALUES (?1, ?2, ?3, '')",
            params![stop_id, saved_path, taken_at],
        )?;

        Ok(PhotoRecord {
            id: conn.last_insert_rowid(),
            stop_id,
            file_path: saved_path,
            taken_at,
            caption: String::new(),
        })
    }

    pub fn load_photos_for_stop(&self, stop_id: i64) -> rusqlite::Result<Vec<PhotoRecord>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, stop_id, file_path, taken_at, caption FROM stop_photos WHERE stop_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![stop_id], |r| {
            Ok(PhotoRecord {
                id: r.get(0)?,
                stop_id: r.get(1)?,
                file_path: r.get(2)?,
                taken_at: r.get(3)?,
                caption: r.get(4)?,
            })
        })?;
        Ok(rows.flatten().collect())
    }

    pub fn delete_photos_for_stop(&self, stop_id: i64) -> rusqlite::Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute("DELETE FROM stop_photos WHERE stop_id = ?1", params![stop_id])?;
        Ok(())
    }

    /// 懒加载并缓存缩略图纹理用于界面渲染
    pub fn get_thumbnail(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.texture_cache.get(path) {
            return Some(tex.clone());
        }
        let img = image::open(path).ok()?;
        let thumb = img.thumbnail(260, 190).to_rgba8();
        let size = [thumb.width() as usize, thumb.height() as usize];
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_raw());
        let handle = ctx.load_texture(path, color_image, egui::TextureOptions::LINEAR);
        self.texture_cache.insert(path.to_string(), handle.clone());
        Some(handle)
    }
}

