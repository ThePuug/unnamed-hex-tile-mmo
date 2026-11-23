//! The store's queries. Each is one transaction or one read, and none
//! reads anything of the game but the values handed it.

use common::Cover;
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
/// every tile, pile and den kept on them, a pile as the JSON of what it
/// holds and a den as its JSON, by its site's `(q, r)`.
pub struct Recalled {
    pub epochs: Vec<(ChunkId, i64)>,
    pub tiles: Vec<(i32, i32, Cover)>,
    pub piles: Vec<(PileKey, Vec<serde_json::Value>)>,
    pub dens: Vec<((i32, i32), serde_json::Value)>,
}

/// Tiles, piles and characters to keep, each under the ownership number
/// its chunk or its account was claimed with. A tile's piles are written
/// whole: one with none left has none kept.
#[derive(Default)]
pub struct Batch {
    pub epochs: Vec<(ChunkId, i64)>,
    pub tiles: Vec<(ChunkId, i32, i32, Cover)>,
    pub piles: Vec<(ChunkId, PileKey, Vec<serde_json::Value>)>,
    /// Each den site written, by its `(q, r)`: its den's JSON, or none
    /// where it has none any more.
    pub dens: Vec<(ChunkId, (i32, i32), Option<String>)>,
    /// Each account written and the number it was claimed with.
    pub accounts: Vec<(i64, i64)>,
    /// Each character written, by its id, as its state's JSON.
    pub characters: Vec<(i64, String)>,
    /// Whether an item appeared, was consumed or changed holder in what
    /// the batch carries: the write then waits on the disk.
    pub economy: bool,
}

/// The chunks and accounts a write named under numbers since raised.
/// Where either holds any, nothing of the batch was written.
#[derive(Debug, Default, PartialEq)]
pub struct Refused {
    pub chunks: Vec<ChunkId>,
    pub accounts: Vec<i64>,
}

impl Refused {
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty() && self.accounts.is_empty()
    }
}

/// What claiming a player's account found: the account, its new ownership
/// number, and its first character's id and state, JSON `null` where it
/// has never been written.
pub struct Entered {
    pub account: i64,
    pub epoch: i64,
    pub character: i64,
    pub state: serde_json::Value,
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
            r#"SELECT q, r, slot, stacks AS "stacks: Json<Vec<serde_json::Value>>" FROM pile WHERE chunk = ANY($1)"#,
            &keys,
        )
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| ((row.q, row.r, row.slot as usize), row.stacks.0))
        .collect();
        let dens = sqlx::query!(r#"SELECT q, r, state AS "state: Json<serde_json::Value>" FROM den WHERE chunk = ANY($1)"#, &keys)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| ((row.q, row.r), row.state.0))
            .collect();
        tx.commit().await?;
        Ok(Recalled { epochs, tiles, piles, dens })
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

    /// Claims the account `provider` knows as `name` for this server,
    /// raising its ownership number, and reads its first character, in one
    /// transaction. A player never seen is given an account, and an
    /// account with no character its first, unwritten.
    pub async fn enter(&self, provider: &str, name: &str) -> Result<Entered, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let known = sqlx::query_scalar!("SELECT account FROM identity WHERE provider = $1 AND name = $2", provider, name)
            .fetch_optional(&mut *tx)
            .await?;
        let account = match known {
            Some(account) => account,
            None => {
                let account = sqlx::query_scalar!("INSERT INTO account DEFAULT VALUES RETURNING id").fetch_one(&mut *tx).await?;
                sqlx::query!("INSERT INTO identity (provider, name, account) VALUES ($1, $2, $3)", provider, name, account)
                    .execute(&mut *tx)
                    .await?;
                account
            }
        };
        let epoch = sqlx::query_scalar!("UPDATE account SET epoch = epoch + 1, holder = $2 WHERE id = $1 RETURNING epoch", account, self.holder)
            .fetch_one(&mut *tx)
            .await?;
        let first = sqlx::query!(
            r#"SELECT id, state AS "state: Json<serde_json::Value>" FROM player_character WHERE account = $1 ORDER BY id LIMIT 1"#,
            account,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let (character, state) = match first {
            Some(row) => (row.id, row.state.0),
            None => {
                let id = sqlx::query_scalar!("INSERT INTO player_character (account, state) VALUES ($1, 'null') RETURNING id", account)
                    .fetch_one(&mut *tx)
                    .await?;
                (id, serde_json::Value::Null)
            }
        };
        tx.commit().await?;
        Ok(Entered { account, epoch, character, state })
    }

    /// Writes `batch` whole where this server still holds every chunk and
    /// account at the number it names. Where it does not, nothing is
    /// written and those it does not hold are returned: half a batch would
    /// keep one side of a move and lose the other. The ownership rows are
    /// locked until the write commits, so a claim made meanwhile waits for
    /// it instead of slipping between check and write.
    pub async fn write(&self, batch: Batch) -> Result<Refused, sqlx::Error> {
        let (keys, epochs): (Vec<i64>, Vec<i64>) = batch.epochs.iter().map(|&(c, e)| (key(c), e)).unzip();
        let mut tx = self.pool.begin().await?;
        if !batch.economy {
            // A crash loses what the flush interval holds anyway; waiting on
            // the disk for each flush buys nothing.
            sqlx::raw_sql("SET LOCAL synchronous_commit = off").execute(&mut *tx).await?;
        }
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
        let (accounts, account_epochs): (Vec<i64>, Vec<i64>) = batch.accounts.iter().copied().unzip();
        let held_accounts: std::collections::HashSet<i64> = sqlx::query_scalar!(
            r#"SELECT a.id AS "id!" FROM account a, UNNEST($1::BIGINT[], $2::BIGINT[]) AS h(id, epoch)
             WHERE a.id = h.id AND a.epoch = h.epoch AND a.holder = $3
             FOR SHARE OF a"#,
            &accounts,
            &account_epochs,
            self.holder,
        )
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();
        let refused = Refused {
            chunks: batch.epochs.iter().map(|&(c, _)| c).filter(|&c| !held.contains(&key(c))).collect(),
            accounts: accounts.iter().copied().filter(|a| !held_accounts.contains(a)).collect(),
        };
        if !refused.is_empty() {
            return Ok(refused);
        }

        let tiles: Vec<_> = batch.tiles.iter().collect();
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

        let piles: Vec<_> = batch.piles.iter().collect();
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
        let den_chunks: Vec<i64> = batch.dens.iter().map(|d| key(d.0)).collect();
        let den_qs: Vec<i32> = batch.dens.iter().map(|d| d.1 .0).collect();
        let den_rs: Vec<i32> = batch.dens.iter().map(|d| d.1 .1).collect();
        let den_states: Vec<Option<String>> = batch.dens.iter().map(|d| d.2.clone()).collect();
        sqlx::query!(
            "DELETE FROM den d USING UNNEST($1::BIGINT[], $2::INT[], $3::INT[]) AS t(chunk, q, r)
             WHERE d.chunk = t.chunk AND d.q = t.q AND d.r = t.r",
            &den_chunks,
            &den_qs,
            &den_rs,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO den (chunk, q, r, state)
             SELECT chunk, q, r, state::JSONB
             FROM UNNEST($1::BIGINT[], $2::INT[], $3::INT[], $4::TEXT[]) AS t(chunk, q, r, state)
             WHERE state IS NOT NULL",
            &den_chunks,
            &den_qs,
            &den_rs,
            &den_states as &[Option<String>],
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "UPDATE player_character c SET state = t.state::JSONB
             FROM UNNEST($1::BIGINT[], $2::TEXT[]) AS t(id, state)
             WHERE c.id = t.id AND c.account = ANY($3)",
            &batch.characters.iter().map(|c| c.0).collect::<Vec<_>>(),
            &batch.characters.iter().map(|c| c.1.clone()).collect::<Vec<_>>(),
            &accounts,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(refused)
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

    /// Opens a scratch database made for one test, schema and all, beside
    /// the server's own store, and drops it after.
    fn scratch(test: impl AsyncFnOnce(Store)) {
        let url = super::super::url();
        bevy::tasks::block_on(async {
            let name = format!("test_{}_{}", std::process::id(), rand::random::<u32>());
            let admin = PgPool::connect(&url).await.expect("the store opens");
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}"))).execute(&admin).await.expect("may create a database");
            let (base, _) = url.rsplit_once('/').expect("the store's URL names a database");
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
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn what_is_written_is_recalled_and_read() {
        scratch(async |store| {
            let here = ChunkId(2, -3);
            let claimed = store.recall(&[here]).await.unwrap();
            assert!(claimed.tiles.is_empty() && claimed.piles.is_empty());
            let stacks = vec![serde_json::json!({"material": 1, "count": 3})];
            let batch = Batch {
                epochs: claimed.epochs.clone(),
                tiles: vec![(here, 40, -60, pine())],
                piles: vec![(here, (40, -60, 4), stacks.clone())],
                ..Default::default()
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
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_den_is_kept_until_its_site_has_none() {
        scratch(async |store| {
            let here = ChunkId(1, 1);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let stands = Batch { epochs: epochs.clone(), dens: vec![(here, (20, 21), Some(r#"{"level":4}"#.into()))], ..Default::default() };
            assert!(store.write(stands).await.unwrap().is_empty());
            let epochs = {
                let again = store.recall(&[here]).await.unwrap();
                assert_eq!(again.dens, vec![((20, 21), serde_json::json!({"level": 4}))]);
                again.epochs
            };
            let gone = Batch { epochs, dens: vec![(here, (20, 21), None)], ..Default::default() };
            assert!(store.write(gone).await.unwrap().is_empty());
            assert!(store.recall(&[here]).await.unwrap().dens.is_empty());
        });
    }

    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_write_under_a_stale_number_is_refused_and_writes_nothing() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let first = store.recall(&[here]).await.unwrap();
            let second = store.recall(&[here]).await.unwrap();
            let stale = Batch { epochs: first.epochs, tiles: vec![(here, 1, 1, pine())], ..Default::default() };
            assert_eq!(store.write(stale).await.unwrap().chunks, vec![here]);
            assert!(store.read((0, 0), (5, 5)).await.unwrap().is_empty());
            let fresh = Batch { epochs: second.epochs, tiles: vec![(here, 1, 1, pine())], ..Default::default() };
            assert!(store.write(fresh).await.unwrap().is_empty());
        });
    }

    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_tiles_piles_are_written_whole() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let stone = vec![serde_json::json!({"material": 4, "count": 2})];
            let both = Batch {
                epochs: epochs.clone(),
                tiles: vec![(here, 3, 3, pine())],
                piles: vec![(here, (3, 3, 1), stone.clone()), (here, (3, 3, 2), stone.clone())],
                ..Default::default()
            };
            store.write(both).await.unwrap();
            let one = Batch { epochs, tiles: vec![(here, 3, 3, pine())], piles: vec![(here, (3, 3, 2), stone.clone())], ..Default::default() };
            store.write(one).await.unwrap();
            assert_eq!(store.recall(&[here]).await.unwrap().piles, vec![((3, 3, 2), stone)]);
        });
    }

    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_player_enters_as_its_character_was_written() {
        scratch(async |store| {
            let first = store.enter("key", "ab").await.unwrap();
            assert!(first.state.is_null(), "a new player's character is unwritten");
            let written = Batch { accounts: vec![(first.account, first.epoch)], characters: vec![(first.character, r#"{"at":3}"#.into())], ..Default::default() };
            assert!(store.write(written).await.unwrap().is_empty());

            let again = store.enter("key", "ab").await.unwrap();
            assert_eq!((again.account, again.character), (first.account, first.character));
            assert_eq!(again.state, serde_json::json!({"at": 3}));
            assert!(again.epoch > first.epoch, "each claim raises the number");
            assert_ne!(store.enter("key", "cd").await.unwrap().account, first.account, "another key is another player");
        });
    }

    /// A batch naming one stale holder writes nothing, the holders it
    /// still has included: a pile and a bag are kept together or not at all.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_batch_with_a_stale_holder_writes_nothing_of_the_rest() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let stale = store.enter("key", "ab").await.unwrap();
            let held = store.enter("key", "ab").await.unwrap();
            let batch = Batch {
                epochs: epochs.clone(),
                tiles: vec![(here, 1, 1, pine())],
                accounts: vec![(stale.account, stale.epoch)],
                characters: vec![(stale.character, "{}".into())],
                economy: true,
                ..Default::default()
            };
            assert_eq!(store.write(batch).await.unwrap(), Refused { chunks: vec![], accounts: vec![stale.account] });
            assert!(store.read((0, 0), (5, 5)).await.unwrap().is_empty());
            assert!(store.enter("key", "ab").await.unwrap().state.is_null());
            assert!(held.epoch > stale.epoch);
        });
    }
}
