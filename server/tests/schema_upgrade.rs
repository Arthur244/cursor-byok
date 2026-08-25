use std::borrow::Cow;

use cursor_server::store::Store;
use sqlx::{migrate::Migrator, sqlite::SqliteConnectOptions, Row};

#[tokio::test]
async fn version_two_database_upgrades_with_cursor_request_mapping() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("upgrade.db");
    let pool = sqlx::SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&database)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    let all = sqlx::migrate!("./migrations");
    let prior = Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|migration| migration.version <= 2)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    prior.run(&pool).await.unwrap();
    drop(pool);

    let store = Store::connect(&format!("sqlite://{}", database.display()))
        .await
        .unwrap();
    let columns = sqlx::query("PRAGMA table_info(runs)")
        .fetch_all(store.pool())
        .await
        .unwrap();

    assert!(columns
        .iter()
        .any(|column| column.get::<String, _>("name") == "cursor_request_id"));
}
