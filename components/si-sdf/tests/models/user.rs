use crate::models::billing_account::new_billing_account;
use crate::{one_time_setup, TestContext};

use si_sdf::data::{NatsTxn, PgTxn};
use si_sdf::models::User;

pub async fn create_user(
    txn: &PgTxn<'_>,
    nats: &NatsTxn,
    name: impl Into<String>,
    email: impl Into<String>,
    billing_account_id: impl Into<String>,
) -> User {
    let name = name.into();
    let email = email.into();
    let billing_account_id = billing_account_id.into();

    let user = User::new(txn, nats, name, email, "superdopestar", billing_account_id)
        .await
        .expect("cannot create user");
    user
}

#[tokio::test]
async fn new() {
    one_time_setup().await.expect("one time setup failed");
    let ctx = TestContext::init().await;
    let (pg, nats_conn, _veritech, _event_log_fs, _secret_key) = ctx.entries();
    let nats = nats_conn.transaction();
    let mut conn = pg.pool.get().await.expect("cannot connect to pg");
    let txn = conn.transaction().await.expect("cannot create txn");
    let ba = new_billing_account(&txn, &nats).await;

    let user = User::new(
        &txn,
        &nats,
        "jesse leach",
        "jesse@killswitch.localdomain",
        "superdopestar",
        &ba.id,
    )
    .await
    .expect("cannot create user");
    assert_eq!(user.name, "jesse leach");
}

#[tokio::test]
async fn verify() {
    one_time_setup().await.expect("one time setup failed");
    let ctx = TestContext::init().await;
    let (pg, nats_conn, _veritech, _event_log_fs, _secret_key) = ctx.entries();
    let nats = nats_conn.transaction();
    let mut conn = pg.pool.get().await.expect("cannot connect to pg");
    let txn = conn.transaction().await.expect("cannot create txn");
    let ba = new_billing_account(&txn, &nats).await;
    let user = User::new(
        &txn,
        &nats,
        "jesse leach",
        "jesse@killswitch.localdomain",
        "superdopestar",
        &ba.id,
    )
    .await
    .expect("cannot create user");
    let verified_password = user
        .verify(&txn, "superdopestar")
        .await
        .expect("failed to verify a password");
    assert!(verified_password, "failed to verify a password");
    let unverified_password = user
        .verify(&txn, "lessdope")
        .await
        .expect("failed to verify a failing password");
    assert_eq!(
        unverified_password, false,
        "failed to verify an invalid password"
    );
}

#[tokio::test]
async fn get() {
    one_time_setup().await.expect("one time setup failed");
    let ctx = TestContext::init().await;
    let (pg, nats_conn, _veritech, _event_log_fs, _secret_key) = ctx.entries();
    let nats = nats_conn.transaction();
    let mut conn = pg.pool.get().await.expect("cannot connect to pg");
    let txn = conn.transaction().await.expect("cannot create txn");
    let ba = new_billing_account(&txn, &nats).await;
    let user = User::new(
        &txn,
        &nats,
        "adam d",
        "adam@killswitch.localdomain",
        "superdopestar",
        &ba.id,
    )
    .await
    .expect("cannot create user");
    assert_eq!(user.name, "adam d");
    let o = User::get(&txn, &user.id).await.expect("cannot get user");
    assert_eq!(o.name, user.name);
    assert_eq!(o.email, user.email);
    assert_eq!(o.id, user.id);
}

#[tokio::test]
async fn get_by_email() {
    one_time_setup().await.expect("one time setup failed");
    let ctx = TestContext::init().await;
    let (pg, nats_conn, _veritech, _event_log_fs, _secret_key) = ctx.entries();
    let nats = nats_conn.transaction();
    let mut conn = pg.pool.get().await.expect("cannot connect to pg");
    let txn = conn.transaction().await.expect("cannot create txn");
    let ba = new_billing_account(&txn, &nats).await;
    let user = User::new(
        &txn,
        &nats,
        "adam d",
        "adam@killswitch.localdomain",
        "superdopestar",
        &ba.id,
    )
    .await
    .expect("cannot create user");
    let o = User::get_by_email(&txn, &user.email, &user.si_storable.billing_account_id)
        .await
        .expect("cannot get user by email");
    assert_eq!(o.name, user.name);
    assert_eq!(o.email, user.email);
    assert_eq!(o.id, user.id);
}

#[tokio::test]
async fn list() {
    one_time_setup().await.expect("one time setup failed");
    let ctx = TestContext::init().await;
    let (pg, nats_conn, _veritech, _event_log_fs, _secret_key) = ctx.entries();
    let nats = nats_conn.transaction();
    let mut conn = pg.pool.get().await.expect("cannot connect to pg");
    let txn = conn.transaction().await.expect("cannot create txn");
    let ba = new_billing_account(&txn, &nats).await;
    let users = [
        ("at the gates", "at@gates.com"),
        ("killswitch engage", "at@engage.com"),
        ("parkway drive", "at@parkway.com"),
    ];
    for (u_name, u_email) in users.iter() {
        let _user = User::new(&txn, &nats, *u_name, *u_email, "sosecure", &ba.id)
            .await
            .expect("cannot create user");
    }
    // List all items
    let results = User::list(&txn, &ba.id, None, None, None, None, None)
        .await
        .expect("user list failed");
    assert_eq!(results.items.len(), 3);
}

#[tokio::test]
async fn save() {
    one_time_setup().await.expect("one time setup failed");
    let ctx = TestContext::init().await;
    let (pg, nats_conn, _veritech, _event_log_fs, _secret_key) = ctx.entries();
    let nats = nats_conn.transaction();
    let mut conn = pg.pool.get().await.expect("cannot connect to pg");
    let txn = conn.transaction().await.expect("cannot create txn");
    let ba = new_billing_account(&txn, &nats).await;
    let user = User::new(
        &txn,
        &nats,
        "adam d",
        "adam@killswitch.localdomain",
        "superdopestar",
        &ba.id,
    )
    .await
    .expect("cannot create user");
    let mut u = User::get(&txn, &user.id).await.expect("cannot get user");
    u.name = String::from("poopy pants");
    u.email = String::from("nope@nope.com");
    let updated_u = u.save(&txn, &nats).await.expect("cannot save user");
    assert_eq!(&updated_u.name, "poopy pants");
    assert_eq!(&updated_u.email, "nope@nope.com");
    let updated_u_get = User::get(&txn, &user.id).await.expect("cannot get user");
    assert_eq!(&updated_u_get.name, "poopy pants");
    assert_eq!(&updated_u_get.email, "nope@nope.com");
}