//! Repeatable storage measurements; never touches the user's workspace.
use std::time::Instant;

use noteon::domain::model::{BlockKind, SaveBatch};
use noteon::domain::virtual_layout::VirtualLayout;
use noteon::storage::Repository;
use rusqlite::{Connection, params};

const PAGE_COUNT: i64 = 10_000;
const BLOCKS_PER_PAGE: i64 = 10;
const LARGE_PAGE_BLOCKS: i64 = 20_000;
const SAMPLES: usize = 31;

fn measure<T>(label: &str, mut operation: impl FnMut() -> T) {
    let mut times = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        std::hint::black_box(operation());
        times.push(start.elapsed());
    }
    times.sort();
    println!(
        "{label:<38} p50 {:>9.3} ms | p95 {:>9.3} ms",
        times[SAMPLES / 2].as_secs_f64() * 1000.0,
        times[SAMPLES * 95 / 100].as_secs_f64() * 1000.0
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("performance.sqlite");
    let mut repository = Repository::open(&path)?;
    let start = Instant::now();
    {
        let mut connection = Connection::open(&path)?;
        let transaction = connection.transaction()?;
        {
            let mut page_insert = transaction.prepare("INSERT INTO pages(title) VALUES(?1)")?;
            let mut block_insert = transaction
                .prepare("INSERT INTO blocks(page_id,position,content) VALUES(?1,?2,?3)")?;
            for page in 0..PAGE_COUNT {
                page_insert.execute([format!("Proje {page}")])?;
                let id = transaction.last_insert_rowid();
                let count = if page == 0 {
                    LARGE_PAGE_BLOCKS
                } else {
                    BLOCKS_PER_PAGE
                };
                for position in 0..count {
                    let content = if page == 0 && position == LARGE_PAGE_BLOCKS - 1 {
                        "Benzersizneedle arama hedefi".to_owned()
                    } else {
                        format!(
                            "Proje {page} icin planlama notu {position}. Yerel performans olcumu."
                        )
                    };
                    block_insert.execute(params![id, position, content])?;
                }
            }
        }
        transaction.commit()?;
    }
    println!(
        "Noteon performance | {PAGE_COUNT} pages | {} blocks | {LARGE_PAGE_BLOCKS} blocks on largest page",
        (PAGE_COUNT - 1) * BLOCKS_PER_PAGE + LARGE_PAGE_BLOCKS
    );
    println!("Fixture creation: {:.2}s\n", start.elapsed().as_secs_f64());
    measure("Page metadata (10,000)", || {
        repository.list_pages().unwrap()
    });
    measure("Document outline (20,000)", || {
        repository.document(1).unwrap()
    });
    measure("Deep block window (128 of 20,000)", || {
        repository.blocks(1, 19_000, 128).unwrap()
    });
    measure("FTS rare term", || {
        repository.search("Benzersizneedle").unwrap()
    });
    measure("FTS common term (50 result cap)", || {
        repository.search("planlama").unwrap()
    });
    let mut edited = repository.blocks(1, 19_999, 1)?.remove(0);
    edited.kind = BlockKind::Todo;
    let mut revision = 0;
    measure("Single block transaction (FULL sync)", || {
        revision += 1;
        edited.content = format!("Benzersizneedle arama hedefi, duzenleme {revision}");
        repository
            .save(&SaveBatch {
                blocks: vec![edited.clone()],
                ..Default::default()
            })
            .unwrap()
    });
    let layout = VirtualLayout::new(std::iter::repeat_n(40.0, LARGE_PAGE_BLOCKS as usize));
    measure("10,000 visible-range lookups", || {
        for index in 0..10_000 {
            std::hint::black_box(layout.row_at((index * 79) as f32));
        }
    });
    assert!(repository.blocks(1, 19_000, 128)?.len() == 128);
    assert!(repository.page(1)?.block_count == LARGE_PAGE_BLOCKS as usize);
    Ok(())
}
