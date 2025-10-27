use chrono::Utc;
use image::{ImageBuffer, Rgba};
use imageproc::drawing::draw_text_mut;
use sqlx::{mysql::MySqlRow, MySql, Pool, Row};
use std::path::PathBuf;

// Use ab_glyph for fonts with imageproc 0.24+
use ab_glyph::FontArc;

pub async fn build_summary_image(pool: &Pool<MySql>, path: &PathBuf) -> Result<(), String> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM countries")
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let top5: Vec<MySqlRow> = sqlx::query(
        "SELECT name, estimated_gdp FROM countries WHERE estimated_gdp IS NOT NULL ORDER BY estimated_gdp DESC LIMIT 5",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut lines: Vec<String> = vec![
        format!("Total countries: {}", total.0),
        "Top 5 by estimated GDP:".into(),
    ];
    for (i, r) in top5.iter().enumerate() {
        let name: String = r.try_get("name").unwrap_or_default();
        let gdp: f64 = r.try_get("estimated_gdp").unwrap_or_default();
        lines.push(format!("{}. {} — {:.2}", i + 1, name, gdp));
    }
    lines.push(format!("Timestamp: {}", Utc::now().to_rfc3339()));

    tokio::task::spawn_blocking({
        let path = path.clone();
        move || {
            // Canvas
            let width = 1000u32;
            let height = 600u32;
            let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_pixel(width, height, Rgba([245, 247, 250, 255]));

            // Load TTF (embedded at compile-time)
            let font_data: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");
            let font = FontArc::try_from_slice(font_data)
                .map_err(|_| "font load failed".to_string())?;

            // ab_glyph uses a plain f32 for pixel scale
            let scale: f32 = 28.0;

            // Draw lines
            let mut y = 40i32;
            for line in lines {
                draw_text_mut(&mut img, Rgba([20, 23, 26, 255]), 40, y, scale, &font, &line);
                y += 40;
            }

            img.save(&path).map_err(|e| e.to_string())?;
            Ok::<(), String>(())
        }
    })
    .await
    .map_err(|e| format!("spawn failed: {:?}", e))??;

    Ok(())
}
