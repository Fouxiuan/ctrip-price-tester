use rusqlite::{Connection, params};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use crate::opencli::HotelSearchResult;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS searched_hotels (
    hotel_id    TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    city_name   TEXT NOT NULL DEFAULT '',
    province_name TEXT NOT NULL DEFAULT '',
    url         TEXT NOT NULL DEFAULT '',
    min_price   INTEGER,
    updated_at  TEXT NOT NULL
);
";

fn db_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
    std::fs::create_dir_all(&dir).map_err(|error| format!("无法创建数据目录：{error}"))?;
    Ok(dir.join("hotels.db"))
}

fn open(app: &AppHandle) -> Result<Connection, String> {
    let conn = Connection::open(db_path(app)?)
        .map_err(|error| format!("无法打开价格记录数据库：{error}"))?;
    conn.execute_batch(SCHEMA)
        .map_err(|error| format!("无法初始化价格记录数据库：{error}"))?;
    Ok(conn)
}

pub fn setup(app: &AppHandle) -> Result<(), String> {
    app.manage(Mutex::new(open(app)?));
    Ok(())
}

fn upsert_hotel(conn: &Connection, hotel: &HotelSearchResult) -> Result<(), String> {
    conn.execute(
        "INSERT INTO searched_hotels (hotel_id, name, city_name, province_name, url, min_price, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
         ON CONFLICT(hotel_id) DO UPDATE SET
             name = excluded.name,
             city_name = excluded.city_name,
             province_name = excluded.province_name,
             url = excluded.url,
             updated_at = excluded.updated_at",
        params![
            hotel.hotel_id,
            hotel.name,
            hotel.city_name,
            hotel.province_name,
            hotel.url,
            chrono::Utc::now().to_rfc3339(),
        ],
    )
    .map_err(|error| format!("写入已搜索酒店失败：{error}"))?;
    Ok(())
}

pub fn upsert_hotels(conn: &Connection, hotels: &[HotelSearchResult]) -> Result<(), String> {
    for hotel in hotels {
        upsert_hotel(conn, hotel)?;
    }
    Ok(())
}

pub fn load_hotels(conn: &Connection) -> Result<Vec<HotelSearchResult>, String> {
    let mut statement = conn
        .prepare(
            "SELECT hotel_id, name, city_name, province_name, url, min_price
             FROM searched_hotels ORDER BY updated_at DESC",
        )
        .map_err(|error| format!("读取已搜索酒店失败：{error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(HotelSearchResult {
                hotel_id: row.get(0)?,
                name: row.get(1)?,
                city_name: row.get(2)?,
                province_name: row.get(3)?,
                url: row.get(4)?,
                min_price: row.get(5)?,
                display_type: String::new(),
                source: String::new(),
            })
        })
        .map_err(|error| format!("读取已搜索酒店失败：{error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取已搜索酒店失败：{error}"))
}

fn update_min_price(conn: &Connection, hotel_id: &str, min_price: i64) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE searched_hotels SET min_price = ?2, updated_at = ?3 WHERE hotel_id = ?1",
            params![hotel_id, min_price, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|error| format!("更新酒店最低价失败：{error}"))?;
    if changed == 0 {
        return Err("该酒店不在已搜索记录中。".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn load_searched_hotels(
    state: State<'_, Mutex<Connection>>,
) -> Result<Vec<HotelSearchResult>, String> {
    let conn = state.lock().await;
    load_hotels(&conn)
}

#[tauri::command]
pub async fn update_hotel_min_price(
    state: State<'_, Mutex<Connection>>,
    hotel_id: String,
    min_price: i64,
) -> Result<(), String> {
    let conn = state.lock().await;
    update_min_price(&conn, &hotel_id, min_price)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opencli::HotelSearchResult;

    fn hotel(id: &str, name: &str) -> HotelSearchResult {
        HotelSearchResult {
            hotel_id: id.to_string(),
            name: name.to_string(),
            city_name: "上海".to_string(),
            province_name: "上海".to_string(),
            url: format!("https://hotels.ctrip.com/hotels/detail/?hotelid={id}"),
            min_price: None,
            display_type: String::new(),
            source: "suggest".to_string(),
        }
    }

    #[test]
    fn persists_hotels_and_min_prices() {
        let conn = Connection::open_in_memory().expect("memory db");
        conn.execute_batch(SCHEMA).expect("schema");

        upsert_hotels(&conn, &[hotel("375539", "上海和平饭店")]).expect("upsert");
        let loaded = load_hotels(&conn).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].hotel_id, "375539");
        assert_eq!(loaded[0].name, "上海和平饭店");
        assert_eq!(loaded[0].min_price, None);

        update_min_price(&conn, "375539", 2493).expect("price");
        let loaded = load_hotels(&conn).expect("load");
        assert_eq!(loaded[0].min_price, Some(2493));
    }

    #[test]
    fn reupsert_keeps_existing_min_price() {
        let conn = Connection::open_in_memory().expect("memory db");
        conn.execute_batch(SCHEMA).expect("schema");

        upsert_hotels(&conn, &[hotel("375539", "上海和平饭店")]).expect("upsert");
        update_min_price(&conn, "375539", 2493).expect("price");
        upsert_hotels(&conn, &[hotel("375539", "上海和平饭店")]).expect("re-upsert");

        let loaded = load_hotels(&conn).expect("load");
        assert_eq!(loaded[0].min_price, Some(2493));
    }

    #[test]
    fn price_update_for_unknown_hotel_is_rejected() {
        let conn = Connection::open_in_memory().expect("memory db");
        conn.execute_batch(SCHEMA).expect("schema");
        assert!(update_min_price(&conn, "999999", 100).is_err());
    }
}
