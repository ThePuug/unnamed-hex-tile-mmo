//! The store's queries. Each is one transaction or one read, and none
//! reads anything of the game but the values handed it.

use common::{Cover, Stack};
use common_bevy::chunk::ChunkId;
use sqlx::{postgres::PgPoolOptions, types::Json, PgPool};

/// A connection pool to the store, and the name this server claims chunks
/// under.
#[derive(Clone)]
pub struct Store {
    pool: PgPool,
    holder: String,
}

/// A pile's place: its tile and slot.
pub type PileKey = (i32, i32, usize);

/// What a claim of chunks found: each chunk's new ownership number, and
/// every tile and pile kept on them.
pub struct Recalled {
    pub epochs: Vec<(ChunkId, i64)>,
    pub tiles: Vec<(i32, i32, Cover)>,
    pub piles: Vec<(PileKey, Vec<Stack>)>,
}

/// Tiles and piles to keep, each under the ownership number its chunk was
/// claimed with. A tile's piles are written whole: one with none left has
/// none kept.
#[derive(Default)]
pub struct Batch {
    pub epochs: Vec<(ChunkId, i64)>,
    pub tiles: Vec<(ChunkId, i32, i32, Cover)>,
    pub piles: Vec<(ChunkId, PileKey, Vec<Stack>)>,
}

/// The one column a chunk is keyed by.
fn key(chunk: ChunkId) -> i64 {
    (chunk.0 as i64) << 32 | chunk.1 as u32 as i64
}

fn chunk(key: i64) -> ChunkId {
    ChunkId((key >> 32) as i32, key as i32)
}

impl Store {
    /// Connects to the store at `url` and brings its schema up to date.
    pub async fn open(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new().max_connections(4).connect(url).await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool, holder: format!("server pid {}", std::process::id()) })
    }

    /// Claims `chunks` for this server, raising each one's ownership number,
    /// and reads everything kept on them, in one transaction.
    pub async fn recall(&self, chunks: &[ChunkId]) -> Result<Recalled, sqlx::Error> {
        let keys: Vec<i64> = chunks.iter().copied().map(key).collect();
        let mut tx = self.pool.begin().await?;
        let epochs = sqlx::query!(
            "INSERT INTO chunk_owner (chunk, epoch, holder)
             SELECT chunk, 1, $2 FROM UNNEST($1::BIGINT[]) AS t(chunk)
             ON CONFLICT (chunk) DO UPDATE SET epoch = chunk_owner.epoch + 1, holder = EXCLUDED.holder
             RETURNING chunk, epoch",
            &keys,
            self.holder,
        )
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| (chunk(row.chunk), row.epoch))
        .collect();
        let tiles = sqlx::query!("SELECT q, r, cover FROM tile_cover WHERE chunk = ANY($1)", &keys)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| (row.q, row.r, Cover::from_bits(row.cover as u32)))
            .collect();
        let piles = sqlx::query!(
            r#"SELECT q, r, slot, stacks AS "stacks: Json<Vec<Stack>>" FROM pile WHERE chunk = ANY($1)"#,
            &keys,
        )
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| ((row.q, row.r, row.slot as usize), row.stacks.0))
        .collect();
        tx.commit().await?;
        Ok(Recalled { epochs, tiles, piles })
    }

    /// Every tile kept with `min.0 <= q <= max.0` and `min.1 <= r <= max.1`,
    /// whoever owns it.
    pub async fn read(&self, min: (i32, i32), max: (i32, i32)) -> Result<Vec<(i32, i32, Cover)>, sqlx::Error> {
        Ok(sqlx::query!(
            "SELECT q, r, cover FROM tile_cover WHERE q BETWEEN $1 AND $2 AND r BETWEEN $3 AND $4",
            min.0,
            max.0,
            min.1,
            max.1,
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| (row.q, row.r, Cover::from_bits(row.cover as u32)))
        .collect())
    }

    /// Writes `batch` where this server still holds each chunk at the number
    /// it names, and returns the chunks it does not, whose part is dropped.
    /// The ownership rows are locked until the write commits, so a claim
    /// made meanwhile waits for it instead of slipping between check and
    /// write.
    pub async fn write(&self, batch: Batch) -> Result<Vec<ChunkId>, sqlx::Error> {
        let (keys, epochs): (Vec<i64>, Vec<i64>) = batch.epochs.iter().map(|&(c, e)| (key(c), e)).unzip();
        let mut tx = self.pool.begin().await?;
        // A crash loses what the flush interval holds anyway; waiting on the
        // disk for each flush buys nothing.
        sqlx::raw_sql("SET LOCAL synchronous_commit = off").execute(&mut *tx).await?;
        let held: std::collections::HashSet<i64> = sqlx::query_scalar!(
            r#"SELECT o.chunk AS "chunk!" FROM chunk_owner o, UNNEST($1::BIGINT[], $2::BIGINT[]) AS h(chunk, epoch)
             WHERE o.chunk = h.chunk AND o.epoch = h.epoch AND o.holder = $3
             FOR SHARE OF o"#,
            &keys,
            &epochs,
            self.holder,
        )
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();

        let tiles: Vec<_> = batch.tiles.iter().filter(|t| held.contains(&key(t.0))).collect();
        let tile_chunks: Vec<i64> = tiles.iter().map(|t| key(t.0)).collect();
        let qs: Vec<i32> = tiles.iter().map(|t| t.1).collect();
        let rs: Vec<i32> = tiles.iter().map(|t| t.2).collect();
        let covers: Vec<i32> = tiles.iter().map(|t| t.3.bits() as i32).collect();
        sqlx::query!(
            "INSERT INTO tile_cover (chunk, q, r, cover)
             SELECT * FROM UNNEST($1::BIGINT[], $2::INT[], $3::INT[], $4::INT[])
             ON CONFLICT (chunk, q, r) DO UPDATE SET cover = EXCLUDED.cover",
            &tile_chunks,
            &qs,
            &rs,
            &covers,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM pile p USING UNNEST($1::BIGINT[], $2::INT[], $3::INT[]) AS t(chunk, q, r)
             WHERE p.chunk = t.chunk AND p.q = t.q AND p.r = t.r",
            &tile_chunks,
            &qs,
            &rs,
        )
        .execute(&mut *tx)
        .await?;

        let piles: Vec<_> = batch.piles.iter().filter(|p| held.contains(&key(p.0))).collect();
        let stacks: Vec<String> = piles.iter().map(|p| serde_json::to_string(&p.2).expect("stacks serialize")).collect();
        sqlx::query!(
            "INSERT INTO pile (chunk, q, r, slot, stacks)
             SELECT chunk, q, r, slot, stacks::JSONB
             FROM UNNEST($1::BIGINT[], $2::INT[], $3::INT[], $4::SMALLINT[], $5::TEXT[]) AS t(chunk, q, r, slot, stacks)",
            &piles.iter().map(|p| key(p.0)).collect::<Vec<_>>(),
            &piles.iter().map(|p| p.1 .0).collect::<Vec<_>>(),
            &piles.iter().map(|p| p.1 .1).collect::<Vec<_>>(),
            &piles.iter().map(|p| p.1 .2 as i16).collect::<Vec<_>>(),
            &stacks,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(batch.epochs.iter().map(|&(c, _)| c).filter(|&c| !held.contains(&key(c))).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_key_round_trips_every_quadrant() {
        for c in [ChunkId(0, 0), ChunkId(-1, 0), ChunkId(0, -1), ChunkId(-7, 12), ChunkId(i32::MIN, i32::MAX), ChunkId(i32::MAX, i32::MIN)] {
            assert_eq!(chunk(key(c)), c);
        }
    }

    /// Opens a scratch database made for one test, schema and all, and
    /// drops it after. None without DATABASE_URL, the server's own store.
    fn scratch(test: impl AsyncFnOnce(Store)) {
        let Ok(url) = std::env::var("DATABASE_URL") else { return };
        bevy::tasks::block_on(async {
            let name = format!("test_{}_{}", std::process::id(), rand::random::<u32>());
            let admin = PgPool::connect(&url).await.expect("the store at DATABASE_URL opens");
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}"))).execute(&admin).await.expect("may create a database");
            let (base, _) = url.rsplit_once('/').expect("DATABASE_URL names a database");
            let store = Store::open(&format!("{base}/{name}")).await.expect("the scratch store opens");
            test(store.clone()).await;
            store.pool.close().await;
            sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE {name}"))).execute(&admin).await.expect("may drop it");
        });
    }

    fn pine() -> Cover {
        Cover::NONE.with(0, common::Content::Pine)
    }

    #[test]
    #[ignore = "needs a PostgreSQL at DATABASE_URL"]
    fn what_is_written_is_recalled_and_read() {
        scratch(async |store| {
            let here = ChunkId(2, -3);
            let claimed = store.recall(&[here]).await.unwrap();
            assert!(claimed.tiles.is_empty() && claimed.piles.is_empty());
            let stacks = vec![Stack { kind: common::Stackable::Material(common::Material::Softwood), count: 3 }];
            let batch = Batch {
                epochs: claimed.epochs.clone(),
                tiles: vec![(here, 40, -60, pine())],
                piles: vec![(here, (40, -60, 4), stacks.clone())],
            };
            assert!(store.write(batch).await.unwrap().is_empty());

            let again = store.recall(&[here]).await.unwrap();
            assert_eq!(again.tiles, vec![(40, -60, pine())]);
            assert_eq!(again.piles, vec![((40, -60, 4), stacks)]);
            assert!(again.epochs[0].1 > claimed.epochs[0].1, "each claim raises the number");
            assert_eq!(store.read((0, -100), (50, 0)).await.unwrap(), vec![(40, -60, pine())]);
            assert!(store.read((41, -100), (50, 0)).await.unwrap().is_empty());
        });
    }

    #[test]
    #[ignore = "needs a PostgreSQL at DATABASE_URL"]
    fn a_write_under_a_stale_number_is_refused_and_writes_nothing() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let first = store.recall(&[here]).await.unwrap();
            let second = store.recall(&[here]).await.unwrap();
            let stale = Batch { epochs: first.epochs, tiles: vec![(here, 1, 1, pine())], piles: vec![] };
            assert_eq!(store.write(stale).await.unwrap(), vec![here]);
            assert!(store.read((0, 0), (5, 5)).await.unwrap().is_empty());
            let fresh = Batch { epochs: second.epochs, tiles: vec![(here, 1, 1, pine())], piles: vec![] };
            assert!(store.write(fresh).await.unwrap().is_empty());
        });
    }

    #[test]
    #[ignore = "needs a PostgreSQL at DATABASE_URL"]
    fn a_tiles_piles_are_written_whole() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let stone = vec![Stack { kind: common::Stackable::Material(common::Material::Limestone), count: 2 }];
            let both = Batch {
                epochs: epochs.clone(),
                tiles: vec![(here, 3, 3, pine())],
                piles: vec![(here, (3, 3, 1), stone.clone()), (here, (3, 3, 2), stone.clone())],
            };
            store.write(both).await.unwrap();
            let one = Batch { epochs, tiles: vec![(here, 3, 3, pine())], piles: vec![(here, (3, 3, 2), stone.clone())] };
            store.write(one).await.unwrap();
            assert_eq!(store.recall(&[here]).await.unwrap().piles, vec![((3, 3, 2), stone)]);
        });
    }
}
