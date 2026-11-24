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

/// Whose a construct is: its builder as a provider's name for a player,
/// or the camp it is of as the tile of that camp's hearth.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltRow {
    pub provider: Option<String>,
    pub name: Option<String>,
    pub camp: Option<(i32, i32)>,
}

/// Every hub kept, as its hearths' tiles, and every player that has
/// played of late, by its provider and name, with its character's home
/// as its JSON.
pub struct Settled {
    pub hubs: Vec<Vec<(i32, i32)>>,
    pub residents: Vec<(String, String, serde_json::Value)>,
}

/// What a claim of chunks found: each chunk's new ownership number, and
/// every tile, pile, den, camp and construct's record kept on them, a
/// pile as the JSON of what it holds, a den as its JSON by its site's
/// `(q, r)` and a camp as its JSON by its hearth's.
pub struct Recalled {
    pub epochs: Vec<(ChunkId, i64)>,
    pub tiles: Vec<(i32, i32, Cover)>,
    pub piles: Vec<(PileKey, Vec<serde_json::Value>)>,
    pub dens: Vec<((i32, i32), serde_json::Value)>,
    pub camps: Vec<((i32, i32), serde_json::Value)>,
    pub constructs: Vec<((i32, i32), BuiltRow)>,
}

/// One economy interaction as the ledger keeps it: so many of `item`
/// from `source` to `dest`, each as its JSON, by the cause of that number
/// and, where an NPC's death caused it, the archetype of that number.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub item: serde_json::Value,
    pub count: i64,
    pub source: serde_json::Value,
    pub dest: serde_json::Value,
    pub cause: i32,
    pub npc: Option<i32>,
}

/// Tiles, piles and characters to keep, each under the ownership number
/// its chunk or its account was claimed with. A tile's piles and its
/// construct's record are written whole: one with none left has none
/// kept.
#[derive(Default)]
pub struct Batch {
    pub epochs: Vec<(ChunkId, i64)>,
    pub tiles: Vec<(ChunkId, i32, i32, Cover)>,
    pub piles: Vec<(ChunkId, PileKey, Vec<serde_json::Value>)>,
    pub constructs: Vec<(ChunkId, (i32, i32), BuiltRow)>,
    /// Every hub as its hearths' tiles, where any changed: they are
    /// written whole.
    pub hubs: Option<Vec<Vec<(i32, i32)>>>,
    /// Each den site written, by its `(q, r)`: its den's JSON, or none
    /// where it has none any more.
    pub dens: Vec<(ChunkId, (i32, i32), Option<String>)>,
    /// Each camp written, by its hearth's `(q, r)`: its JSON, or none
    /// where the hearth has none any more.
    pub camps: Vec<(ChunkId, (i32, i32), Option<String>)>,
    /// Each account written and the number it was claimed with.
    pub accounts: Vec<(i64, i64)>,
    /// Each character written, by its id, as its state's JSON.
    pub characters: Vec<(i64, String)>,
    /// The economy interactions that made the holders written what they
    /// are, in the order they happened: appended with them or not at all.
    pub ledger: Vec<Entry>,
    /// Whether an item appeared, was consumed or changed holder in what
    /// the batch carries: the write then waits on the disk.
    pub economy: bool,
    /// The calendar's moment as the batch was made, in its milliseconds:
    /// when each character written was last in the world.
    pub when: i64,
    /// The calendar as an admin left it, where it changed: how far it
    /// stands from the wall clock, and the moment it is held at.
    pub calendar: Option<(i64, Option<i64>)>,
    /// Each chunk a map charted, by the map's number, as its leaf is
    /// kept; the ground as it lies now under map 0.
    pub leaves: Vec<(i64, ChunkId, Vec<u8>)>,
    /// Each map used up, by its number: nothing it charted is kept.
    pub spent: Vec<i64>,
    /// Each depot written, by the tile its hub is known by: what it
    /// holds, as the JSON of its stacks.
    pub depots: Vec<((i32, i32), String)>,
    /// Each setting an admin changed, by its number, and its value.
    pub settings: Vec<(i32, i32)>,
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

/// The tables a rollback leaves as they are, each for its reason: the
/// journal itself and the migrations' own record; who owns a chunk and an
/// account, since a rollback moves no ownership; and which account an
/// identity names, since it forgets no player; and the settings, since
/// how the server is tuned is not the world's. Every other table the
/// store holds journals its changes, or the store does not open.
const UNJOURNALED: [&str; 6] = ["history", "_sqlx_migrations", "chunk_owner", "account", "identity", "setting"];

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
        let store = Self { pool, holder: format!("server pid {}", std::process::id()) };
        let outside = store.unjournaled().await?;
        assert!(
            outside.is_empty(),
            "the store keeps {outside:?} outside its journal, so a rollback would leave them as they are: give each the `journal` trigger in a migration, or name it in `store::UNJOURNALED` with why",
        );
        Ok(store)
    }

    /// The tables the store holds that neither journal their changes nor
    /// are named in [`UNJOURNALED`]: what a rollback would not put back.
    /// The store opens only where there are none.
    async fn unjournaled(&self) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT c.relname::TEXT FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relname <> ALL($1)
               AND NOT EXISTS (SELECT 1 FROM pg_trigger t WHERE t.tgrelid = c.oid AND t.tgname = 'journal')
             ORDER BY 1",
        )
        .bind(UNJOURNALED.map(String::from).to_vec())
        .fetch_all(&self.pool)
        .await
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
            .map(|row| (row.q, row.r, Cover::from_bits(row.cover as u64)))
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
        let camps = sqlx::query!(r#"SELECT q, r, state AS "state: Json<serde_json::Value>" FROM camp WHERE chunk = ANY($1)"#, &keys)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| ((row.q, row.r), row.state.0))
            .collect();
        let constructs = sqlx::query!("SELECT q, r, provider, name, camp_q, camp_r FROM construct WHERE chunk = ANY($1)", &keys)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| ((row.q, row.r), BuiltRow { provider: row.provider, name: row.name, camp: row.camp_q.zip(row.camp_r) }))
            .collect();
        tx.commit().await?;
        Ok(Recalled { epochs, tiles, piles, dens, camps, constructs })
    }

    /// Rolls everything kept of the world back `ms` milliseconds from now:
    /// on the calendar, which reads `when`, where `game`; on the real
    /// clock otherwise. The calendar is set back as far, so the world
    /// stands again at that moment. Returns how many changes it undid.
    /// What a running server holds in memory is then older than nothing:
    /// it must write no more and start again.
    pub async fn roll_back(&self, game: bool, ms: i64, when: i64) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let undone = if game {
            sqlx::query_scalar!("SELECT roll_back(NULL, $1)", when - ms).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar!("SELECT roll_back(now() - make_interval(secs => $1::DOUBLE PRECISION / 1000), NULL)", ms as f64).fetch_one(&mut *tx).await?
        };
        sqlx::query_scalar!("SELECT set_config('mmo.when', $1, TRUE)", (when - ms).to_string()).fetch_one(&mut *tx).await?;
        sqlx::query!(
            "INSERT INTO calendar (shift, held) VALUES (-$1::BIGINT, NULL)
             ON CONFLICT (one) DO UPDATE SET shift = calendar.shift - $1, held = calendar.held - $1",
            ms,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(undone.unwrap_or(0))
    }

    /// A run of numbers no map has and none will be given again: the
    /// first, and how many.
    pub async fn numbers(&self) -> Result<(i64, i64), sqlx::Error> {
        let first = sqlx::query_scalar!(r#"SELECT nextval('map_number') AS "first!""#).fetch_one(&self.pool).await?;
        Ok((first, 1024))
    }

    /// Every chunk every map has charted, by the map's number, as its
    /// leaf is kept; the ground as it lies now under map 0.
    pub async fn charts(&self) -> Result<Vec<(i64, ChunkId, Vec<u8>)>, sqlx::Error> {
        Ok(sqlx::query!("SELECT map, chunk, tiles FROM chart_leaf").fetch_all(&self.pool).await?.into_iter().map(|row| (row.map, chunk(row.chunk), row.tiles)).collect())
    }

    /// Every depot kept, by the tile its hub is known by, with what it
    /// holds.
    pub async fn depots(&self) -> Result<Vec<((i32, i32), Vec<serde_json::Value>)>, sqlx::Error> {
        Ok(sqlx::query!(r#"SELECT q, r, stacks AS "stacks: Json<Vec<serde_json::Value>>" FROM depot"#)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| ((row.q, row.r), row.stacks.0))
            .collect())
    }

    /// Every setting kept, by its number, with its value: those an admin
    /// has changed.
    pub async fn settings(&self) -> Result<Vec<(i32, i32)>, sqlx::Error> {
        Ok(sqlx::query!("SELECT id, value FROM setting").fetch_all(&self.pool).await?.into_iter().map(|row| (row.id, row.value)).collect())
    }

    /// The calendar as an admin left it: how far it stands from the wall
    /// clock, in milliseconds, and the moment it is held at. None where
    /// it has never been moved.
    pub async fn calendar(&self) -> Result<Option<(i64, Option<i64>)>, sqlx::Error> {
        Ok(sqlx::query!("SELECT shift, held FROM calendar").fetch_optional(&self.pool).await?.map(|row| (row.shift, row.held)))
    }

    /// Every hub kept, and every player whose character was in the world
    /// in the week before the calendar's moment `when`, with its home.
    pub async fn settled(&self, when: i64) -> Result<Settled, sqlx::Error> {
        let hubs = sqlx::query_scalar!(r#"SELECT hearths AS "hearths: Json<Vec<(i32, i32)>>" FROM hub ORDER BY id"#)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|hearths| hearths.0)
            .collect();
        let residents = sqlx::query!(
            r#"SELECT DISTINCT ON (c.id) i.provider, i.name, c.state -> 'home' AS "home?: Json<serde_json::Value>"
             FROM player_character c JOIN identity i ON i.account = c.account
             WHERE c.seen > $1::BIGINT - 604800000 AND c.state <> 'null'::JSONB
             ORDER BY c.id"#,
            when,
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| (row.provider, row.name, row.home.map_or(serde_json::Value::Null, |home| home.0)))
        .collect();
        Ok(Settled { hubs, residents })
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
        .map(|row| (row.q, row.r, Cover::from_bits(row.cover as u64)))
        .collect())
    }

    /// Claims the account `provider` knows as `name` for this server,
    /// raising its ownership number, and reads its first character, in one
    /// transaction. A player never seen is given an account, and an
    /// account with no character its first, unwritten. Its character was
    /// last in the world at the calendar's moment `when`.
    pub async fn enter(&self, provider: &str, name: &str, when: i64) -> Result<Entered, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query_scalar!("SELECT set_config('mmo.when', $1, TRUE)", when.to_string()).fetch_one(&mut *tx).await?;
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
            Some(row) => {
                sqlx::query!("UPDATE player_character SET seen = $2 WHERE id = $1", row.id, when).execute(&mut *tx).await?;
                (row.id, row.state.0)
            }
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
        // What the journal stamps each change with
        sqlx::query_scalar!("SELECT set_config('mmo.when', $1, TRUE)", batch.when.to_string()).fetch_one(&mut *tx).await?;
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
        let covers: Vec<i64> = tiles.iter().map(|t| t.3.bits() as i64).collect();
        sqlx::query!(
            "INSERT INTO tile_cover (chunk, q, r, cover)
             SELECT * FROM UNNEST($1::BIGINT[], $2::INT[], $3::INT[], $4::BIGINT[])
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
        sqlx::query!(
            "DELETE FROM construct c USING UNNEST($1::BIGINT[], $2::INT[], $3::INT[]) AS t(chunk, q, r)
             WHERE c.chunk = t.chunk AND c.q = t.q AND c.r = t.r",
            &tile_chunks,
            &qs,
            &rs,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO construct (chunk, q, r, provider, name, camp_q, camp_r)
             SELECT * FROM UNNEST($1::BIGINT[], $2::INT[], $3::INT[], $4::TEXT[], $5::TEXT[], $6::INT[], $7::INT[])",
            &batch.constructs.iter().map(|c| key(c.0)).collect::<Vec<_>>(),
            &batch.constructs.iter().map(|c| c.1 .0).collect::<Vec<_>>(),
            &batch.constructs.iter().map(|c| c.1 .1).collect::<Vec<_>>(),
            &batch.constructs.iter().map(|c| c.2.provider.clone()).collect::<Vec<_>>() as &[Option<String>],
            &batch.constructs.iter().map(|c| c.2.name.clone()).collect::<Vec<_>>() as &[Option<String>],
            &batch.constructs.iter().map(|c| c.2.camp.map(|camp| camp.0)).collect::<Vec<_>>() as &[Option<i32>],
            &batch.constructs.iter().map(|c| c.2.camp.map(|camp| camp.1)).collect::<Vec<_>>() as &[Option<i32>],
        )
        .execute(&mut *tx)
        .await?;
        if let Some(hubs) = &batch.hubs {
            sqlx::query!("DELETE FROM hub").execute(&mut *tx).await?;
            sqlx::query!(
                "INSERT INTO hub (hearths) SELECT hearths::JSONB FROM UNNEST($1::TEXT[]) WITH ORDINALITY AS t(hearths, place) ORDER BY place",
                &hubs.iter().map(|hearths| serde_json::to_string(hearths).expect("hearths serialize")).collect::<Vec<_>>(),
            )
            .execute(&mut *tx)
            .await?;
        }
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
        let camp_chunks: Vec<i64> = batch.camps.iter().map(|c| key(c.0)).collect();
        let camp_qs: Vec<i32> = batch.camps.iter().map(|c| c.1 .0).collect();
        let camp_rs: Vec<i32> = batch.camps.iter().map(|c| c.1 .1).collect();
        let camp_states: Vec<Option<String>> = batch.camps.iter().map(|c| c.2.clone()).collect();
        sqlx::query!(
            "DELETE FROM camp c USING UNNEST($1::BIGINT[], $2::INT[], $3::INT[]) AS t(chunk, q, r)
             WHERE c.chunk = t.chunk AND c.q = t.q AND c.r = t.r",
            &camp_chunks,
            &camp_qs,
            &camp_rs,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO camp (chunk, q, r, state)
             SELECT chunk, q, r, state::JSONB
             FROM UNNEST($1::BIGINT[], $2::INT[], $3::INT[], $4::TEXT[]) AS t(chunk, q, r, state)
             WHERE state IS NOT NULL",
            &camp_chunks,
            &camp_qs,
            &camp_rs,
            &camp_states as &[Option<String>],
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "UPDATE player_character c SET state = t.state::JSONB, seen = $4
             FROM UNNEST($1::BIGINT[], $2::TEXT[]) AS t(id, state)
             WHERE c.id = t.id AND c.account = ANY($3)",
            &batch.characters.iter().map(|c| c.0).collect::<Vec<_>>(),
            &batch.characters.iter().map(|c| c.1.clone()).collect::<Vec<_>>(),
            &accounts,
            batch.when,
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO chart_leaf (map, chunk, tiles)
             SELECT * FROM UNNEST($1::BIGINT[], $2::BIGINT[], $3::BYTEA[])
             ON CONFLICT (map, chunk) DO UPDATE SET tiles = EXCLUDED.tiles",
            &batch.leaves.iter().map(|leaf| leaf.0).collect::<Vec<_>>(),
            &batch.leaves.iter().map(|leaf| key(leaf.1)).collect::<Vec<_>>(),
            &batch.leaves.iter().map(|leaf| leaf.2.clone()).collect::<Vec<_>>(),
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!("DELETE FROM chart_leaf WHERE map = ANY($1)", &batch.spent).execute(&mut *tx).await?;
        sqlx::query!(
            "INSERT INTO depot (q, r, stacks)
             SELECT q, r, stacks::JSONB FROM UNNEST($1::INT[], $2::INT[], $3::TEXT[]) AS t(q, r, stacks)
             ON CONFLICT (q, r) DO UPDATE SET stacks = EXCLUDED.stacks",
            &batch.depots.iter().map(|depot| depot.0 .0).collect::<Vec<_>>(),
            &batch.depots.iter().map(|depot| depot.0 .1).collect::<Vec<_>>(),
            &batch.depots.iter().map(|depot| depot.1.clone()).collect::<Vec<_>>(),
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO setting (id, value)
             SELECT id, value FROM UNNEST($1::INT[], $2::INT[]) AS t(id, value)
             ON CONFLICT (id) DO UPDATE SET value = EXCLUDED.value",
            &batch.settings.iter().map(|setting| setting.0).collect::<Vec<_>>(),
            &batch.settings.iter().map(|setting| setting.1).collect::<Vec<_>>(),
        )
        .execute(&mut *tx)
        .await?;
        if let Some((shift, held)) = batch.calendar {
            sqlx::query!(
                "INSERT INTO calendar (shift, held) VALUES ($1, $2)
                 ON CONFLICT (one) DO UPDATE SET shift = EXCLUDED.shift, held = EXCLUDED.held",
                shift,
                held,
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            "INSERT INTO ledger (item, count, source, dest, cause, npc)
             SELECT item::JSONB, count, source::JSONB, dest::JSONB, cause, npc
             FROM UNNEST($1::TEXT[], $2::BIGINT[], $3::TEXT[], $4::TEXT[], $5::INT[], $6::INT[])
                  WITH ORDINALITY AS t(item, count, source, dest, cause, npc, place)
             ORDER BY place",
            &batch.ledger.iter().map(|e| e.item.to_string()).collect::<Vec<_>>(),
            &batch.ledger.iter().map(|e| e.count).collect::<Vec<_>>(),
            &batch.ledger.iter().map(|e| e.source.to_string()).collect::<Vec<_>>(),
            &batch.ledger.iter().map(|e| e.dest.to_string()).collect::<Vec<_>>(),
            &batch.ledger.iter().map(|e| e.cause).collect::<Vec<_>>(),
            &batch.ledger.iter().map(|e| e.npc).collect::<Vec<_>>() as &[Option<i32>],
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

    /// A construct's record is kept with its tile and goes with it; the
    /// hubs are written whole; and a player that has played is settled
    /// where its character's home says.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn constructs_hubs_and_homes_are_kept() {
        scratch(async |store| {
            let here = ChunkId(1, 1);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let wall = Cover::NONE.with_boulder(0);
            let whose = BuiltRow { provider: Some("key".into()), name: Some("ab".into()), camp: None };
            let hubs = vec![vec![(-2, 5), (-1, 4)], vec![(20, 21)]];
            let player = store.enter("key", "ab", 5_000).await.unwrap();
            let batch = Batch {
                epochs: epochs.clone(),
                tiles: vec![(here, 20, 21, wall)],
                constructs: vec![(here, (20, 21), whose.clone())],
                hubs: Some(hubs.clone()),
                accounts: vec![(player.account, player.epoch)],
                characters: vec![(player.character, r#"{"home":{"q":-1,"r":4,"z":7}}"#.into())],
                when: 6_000,
                calendar: Some((-7_200_000, Some(9_000))),
                ..Default::default()
            };
            assert!(store.write(batch).await.unwrap().is_empty());
            let again = store.recall(&[here]).await.unwrap();
            assert_eq!((again.tiles, again.constructs), (vec![(20, 21, wall)], vec![((20, 21), whose)]));
            let settled = store.settled(6_000 + 604_799_999).await.unwrap();
            assert!(store.settled(6_000 + 604_800_000).await.unwrap().residents.is_empty(), "a week on the calendar, it has not played of late");
            assert_eq!(store.calendar().await.unwrap(), Some((-7_200_000, Some(9_000))));
            assert_eq!(settled.hubs, hubs);
            assert_eq!(settled.residents, vec![("key".into(), "ab".into(), serde_json::json!({"q": -1, "r": 4, "z": 7}))]);
            let down = Batch { epochs: again.epochs, tiles: vec![(here, 20, 21, Cover::NONE)], ..Default::default() };
            assert!(store.write(down).await.unwrap().is_empty());
            assert!(store.recall(&[here]).await.unwrap().constructs.is_empty());
        });
    }

    /// A camp is kept until its hearth has none, and what its settler
    /// raised names the camp and no player.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_camp_is_kept_with_what_its_settler_raised() {
        scratch(async |store| {
            let here = ChunkId(1, 1);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let hubs = BuiltRow { provider: None, name: None, camp: Some((20, 21)) };
            let stands = Batch {
                epochs: epochs.clone(),
                tiles: vec![(here, 22, 21, pine())],
                constructs: vec![(here, (22, 21), hubs.clone())],
                camps: vec![(here, (20, 21), Some(r#"{"arrived":true}"#.into()))],
                ..Default::default()
            };
            assert!(store.write(stands).await.unwrap().is_empty());
            let again = store.recall(&[here]).await.unwrap();
            assert_eq!(again.camps, vec![((20, 21), serde_json::json!({"arrived": true}))]);
            assert_eq!(again.constructs, vec![((22, 21), hubs)]);
            let gone = Batch { epochs: again.epochs, camps: vec![(here, (20, 21), None)], ..Default::default() };
            assert!(store.write(gone).await.unwrap().is_empty());
            assert!(store.recall(&[here]).await.unwrap().camps.is_empty());
        });
    }

    /// What a map charted is kept under its number until the map is
    /// used up, and each run of numbers is past the last.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_maps_leaves_are_kept_until_it_is_spent() {
        scratch(async |store| {
            let (first, count) = store.numbers().await.unwrap();
            assert_eq!(store.numbers().await.unwrap().0, first + count);
            let here = ChunkId(-3, 8);
            let charted = Batch { leaves: vec![(first, here, vec![1, 2, 3]), (0, here, vec![4]), (first + 1, here, vec![5])], ..Default::default() };
            assert!(store.write(charted).await.unwrap().is_empty());
            let over = Batch { leaves: vec![(first, here, vec![9])], spent: vec![first + 1], ..Default::default() };
            assert!(store.write(over).await.unwrap().is_empty());
            let mut kept = store.charts().await.unwrap();
            kept.sort_by_key(|leaf| leaf.0);
            assert_eq!(kept, vec![(0, here, vec![4]), (first, here, vec![9])]);
        });
    }

    /// A depot is kept as it was last written.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_depot_is_kept_as_last_written() {
        scratch(async |store| {
            let first = Batch { depots: vec![((7, -3), r#"[{"material":1,"count":2}]"#.into())], ..Default::default() };
            assert!(store.write(first).await.unwrap().is_empty());
            let more = Batch { depots: vec![((7, -3), r#"[{"material":1,"count":9}]"#.into())], ..Default::default() };
            assert!(store.write(more).await.unwrap().is_empty());
            assert_eq!(store.depots().await.unwrap(), vec![((7, -3), vec![serde_json::json!({"material": 1, "count": 9})])]);
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

    /// Every entry kept, oldest first.
    async fn ledger(store: &Store) -> Vec<Entry> {
        use sqlx::Row;
        sqlx::query("SELECT item, count, source, dest, cause, npc FROM ledger ORDER BY id")
            .fetch_all(&store.pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| Entry {
                item: row.get::<Json<serde_json::Value>, _>("item").0,
                count: row.get("count"),
                source: row.get::<Json<serde_json::Value>, _>("source").0,
                dest: row.get::<Json<serde_json::Value>, _>("dest").0,
                cause: row.get("cause"),
                npc: row.get("npc"),
            })
            .collect()
    }

    /// The ledger is appended in the order its entries happened, with the
    /// holders they changed; a refused batch appends nothing; and what
    /// the world gave less what it took back is what is in play.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn the_ledger_is_appended_with_its_holders_or_not_at_all() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let first = store.recall(&[here]).await.unwrap().epochs;
            let hide = serde_json::json!({"material": 6});
            let (world, pile, bag) = (serde_json::json!("world"), serde_json::json!({"pile": [1, 1, 3]}), serde_json::json!({"character": 9}));
            let entry = |count, source: &serde_json::Value, dest: &serde_json::Value, cause, npc| Entry { item: hide.clone(), count, source: source.clone(), dest: dest.clone(), cause, npc };
            let moves = vec![entry(2, &world, &pile, 7, Some(1)), entry(1, &pile, &bag, 2, None)];
            let batch = |epochs| Batch { epochs, tiles: vec![(here, 1, 1, pine())], ledger: moves.clone(), economy: true, ..Default::default() };

            let held = store.recall(&[here]).await.unwrap().epochs;
            assert_eq!(store.write(batch(first)).await.unwrap().chunks, vec![here]);
            assert!(ledger(&store).await.is_empty(), "a refused batch appends nothing");
            assert!(store.write(batch(held)).await.unwrap().is_empty());
            assert_eq!(ledger(&store).await, moves);

            let in_play: i64 = sqlx::query_scalar(
                "SELECT (SUM(count) FILTER (WHERE source = '\"world\"') - COALESCE(SUM(count) FILTER (WHERE dest = '\"world\"'), 0))::BIGINT FROM ledger",
            )
            .fetch_one(&store.pool)
            .await
            .unwrap();
            assert_eq!(in_play, 2);
        });
    }

    /// A setting written is kept, and written again takes its new value.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_setting_written_is_kept_as_it_was_last_set() {
        scratch(async |store| {
            assert!(store.settings().await.unwrap().is_empty());
            assert!(store.write(Batch { settings: vec![(1, 4), (3, 100)], ..Default::default() }).await.unwrap().is_empty());
            assert!(store.write(Batch { settings: vec![(1, 7)], ..Default::default() }).await.unwrap().is_empty());
            let mut kept = store.settings().await.unwrap();
            kept.sort();
            assert_eq!(kept, vec![(1, 7), (3, 100)]);
        });
    }

    /// A table kept outside the journal is found, and a change to a
    /// journaled one that names no moment of the calendar is refused.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn nothing_kept_changes_outside_the_journal() {
        scratch(async |store| {
            assert!(store.unjournaled().await.unwrap().is_empty());
            sqlx::query("CREATE TABLE stray (n INT)").execute(&store.pool).await.unwrap();
            assert_eq!(store.unjournaled().await.unwrap(), vec!["stray".to_string()]);
            let unsaid = sqlx::query("INSERT INTO tile_cover VALUES (1, 2, 3, 0)").execute(&store.pool).await;
            assert!(unsaid.is_err_and(|e| e.to_string().contains("names no moment")), "a write that does not say when is refused");
        });
    }

    /// A rollback puts back everything kept as it was at the moment
    /// asked for, on either clock: a tile and its pile changed since, a
    /// tile first changed since, a character, the ledger's entries; and
    /// sets the calendar back as far. What was undone is not undone
    /// again.
    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_rollback_puts_the_world_back_as_it_was() {
        scratch(async |store| {
            let here = ChunkId(0, 0);
            let epochs = store.recall(&[here]).await.unwrap().epochs;
            let player = store.enter("key", "ab", 1_000).await.unwrap();
            let hide = serde_json::json!({"material": 6});
            let entry = |count| Entry { item: hide.clone(), count, source: serde_json::json!("world"), dest: serde_json::json!({"pile": [3, 3, 1]}), cause: 7, npc: Some(1) };
            let pile = |count: u32| vec![serde_json::json!({"material": 6, "count": count})];
            let at = |when: i64, cover: Cover, held: u32, bag: &str, tiles: Vec<(ChunkId, i32, i32, Cover)>| Batch {
                epochs: epochs.clone(),
                tiles: [vec![(here, 3, 3, cover)], tiles].concat(),
                piles: vec![(here, (3, 3, 1), pile(held))],
                accounts: vec![(player.account, player.epoch)],
                characters: vec![(player.character, bag.into())],
                ledger: vec![entry(held as i64)],
                when,
                ..Default::default()
            };
            assert!(store.write(at(2_000, pine(), 1, r#"{"at":1}"#, vec![])).await.unwrap().is_empty());
            assert!(store.write(at(8_000, Cover::NONE, 5, r#"{"at":2}"#, vec![(here, 4, 4, pine())])).await.unwrap().is_empty());

            // Six seconds of the calendar back from 9000: to after the first write
            assert!(store.roll_back(true, 6_000, 9_000).await.unwrap() > 0);
            let then = store.recall(&[here]).await.unwrap();
            assert_eq!(then.tiles, vec![(3, 3, pine())], "the tile as it was, and the one changed since unchanged again");
            assert_eq!(then.piles, vec![((3, 3, 1), pile(1))]);
            assert_eq!(store.enter("key", "ab", 3_000).await.unwrap().state, serde_json::json!({"at": 1}));
            assert_eq!(ledger(&store).await, vec![entry(1)]);
            assert_eq!(store.calendar().await.unwrap(), Some((-6_000, None)));
            assert_eq!(store.roll_back(true, 100, 3_100).await.unwrap(), 0, "nothing is undone twice");

            // On the real clock, far enough back takes everything
            assert!(store.roll_back(false, 3_600_000, 3_100).await.unwrap() > 0);
            let first = store.recall(&[here]).await.unwrap();
            assert!(first.tiles.is_empty() && first.piles.is_empty());
            assert!(ledger(&store).await.is_empty());
        });
    }

    #[test]
    #[ignore = "needs the PostgreSQL the server keeps to"]
    fn a_player_enters_as_its_character_was_written() {
        scratch(async |store| {
            let first = store.enter("key", "ab", 0).await.unwrap();
            assert!(first.state.is_null(), "a new player's character is unwritten");
            let written = Batch { accounts: vec![(first.account, first.epoch)], characters: vec![(first.character, r#"{"at":3}"#.into())], ..Default::default() };
            assert!(store.write(written).await.unwrap().is_empty());

            let again = store.enter("key", "ab", 0).await.unwrap();
            assert_eq!((again.account, again.character), (first.account, first.character));
            assert_eq!(again.state, serde_json::json!({"at": 3}));
            assert!(again.epoch > first.epoch, "each claim raises the number");
            assert_ne!(store.enter("key", "cd", 0).await.unwrap().account, first.account, "another key is another player");
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
            let stale = store.enter("key", "ab", 0).await.unwrap();
            let held = store.enter("key", "ab", 0).await.unwrap();
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
            assert!(store.enter("key", "ab", 0).await.unwrap().state.is_null());
            assert!(held.epoch > stale.epoch);
        });
    }
}
