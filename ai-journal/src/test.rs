#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env, String};

const XLM: i128 = 10_000_000; // 1 XLM = 10^7 stroops

struct Fixture<'a> {
    env: Env,
    journal: SubSpecieJournalClient<'a>,
    token: TokenClient<'a>,
    curator: Address,
    agent: Address,
}

fn setup() -> Fixture<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let curator = Address::generate(&env);
    let agent = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token = TokenClient::new(&env, &sac.address());
    StellarAssetClient::new(&env, &sac.address()).mint(&curator, &(1_000 * XLM));

    let contract_id = env.register(
        SubSpecieJournal,
        (
            curator.clone(),
            sac.address(),
            String::from_str(&env, "Sub Specie"),
            String::from_str(&env, "anchor-tx-placeholder"),
            String::from_str(&env, "guide-tx-placeholder"),
        ),
    );
    let journal = SubSpecieJournalClient::new(&env, &contract_id);

    Fixture {
        env,
        journal,
        token,
        curator,
        agent,
    }
}

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

#[test]
fn full_cycle_submit_accept_close() {
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 1_000;

    // 회차 개시: 고료 10 XLM × 2편 = 20 XLM 에스크로
    let round_id = f
        .journal
        .open_round(&(10 * XLM), &deadline, &2, &s(&f.env, "call-tx"));
    assert_eq!(round_id, 0);
    assert_eq!(f.token.balance(&f.journal.address), 20 * XLM);
    assert_eq!(f.token.balance(&f.curator), 980 * XLM);

    // 투고
    let sub_id = f.journal.submit(
        &round_id,
        &f.agent,
        &s(&f.env, "manuscript-tx"),
        &s(&f.env, "claude-fable-5"),
    );
    assert_eq!(sub_id, 0);
    assert_eq!(f.journal.get_round(&round_id).submitted, 1);

    // 채택 → 고료 자동 전송
    f.journal.accept(&round_id, &sub_id);
    assert_eq!(f.token.balance(&f.agent), 10 * XLM);
    assert_eq!(f.token.balance(&f.journal.address), 10 * XLM);
    let sub = f.journal.get_submission(&round_id, &sub_id);
    assert!(sub.accepted);
    assert_eq!(f.journal.get_round(&round_id).accepted, 1);

    // 종료 → 잔여 에스크로 10 XLM 환급
    f.journal.close_round(&round_id);
    assert_eq!(f.token.balance(&f.journal.address), 0);
    assert_eq!(f.token.balance(&f.curator), 990 * XLM);
    assert!(f.journal.get_round(&round_id).closed);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn submit_after_deadline_fails() {
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 100;
    let round_id = f
        .journal
        .open_round(&XLM, &deadline, &1, &s(&f.env, "call-tx"));

    f.env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    f.journal.submit(
        &round_id,
        &f.agent,
        &s(&f.env, "m-tx"),
        &s(&f.env, "model"),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn submit_after_close_fails() {
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 1_000;
    let round_id = f
        .journal
        .open_round(&XLM, &deadline, &1, &s(&f.env, "call-tx"));
    f.journal.close_round(&round_id);
    f.journal.submit(
        &round_id,
        &f.agent,
        &s(&f.env, "m-tx"),
        &s(&f.env, "model"),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn accept_beyond_quota_fails() {
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 1_000;
    let round_id = f
        .journal
        .open_round(&XLM, &deadline, &1, &s(&f.env, "call-tx"));

    let agent2 = Address::generate(&f.env);
    let s1 = f
        .journal
        .submit(&round_id, &f.agent, &s(&f.env, "m1"), &s(&f.env, "model"));
    let s2 = f
        .journal
        .submit(&round_id, &agent2, &s(&f.env, "m2"), &s(&f.env, "model"));

    f.journal.accept(&round_id, &s1);
    f.journal.accept(&round_id, &s2);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn double_accept_fails() {
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 1_000;
    let round_id = f
        .journal
        .open_round(&XLM, &deadline, &2, &s(&f.env, "call-tx"));
    let sub_id = f
        .journal
        .submit(&round_id, &f.agent, &s(&f.env, "m"), &s(&f.env, "model"));
    f.journal.accept(&round_id, &sub_id);
    f.journal.accept(&round_id, &sub_id);
}

#[test]
fn acceptance_allowed_after_deadline() {
    // 마감은 '투고' 마감이다. 심사(채택)는 마감 후에도 회차 종료 전까지 가능.
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 100;
    let round_id = f
        .journal
        .open_round(&XLM, &deadline, &1, &s(&f.env, "call-tx"));
    let sub_id = f
        .journal
        .submit(&round_id, &f.agent, &s(&f.env, "m"), &s(&f.env, "model"));

    f.env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    f.journal.accept(&round_id, &sub_id);
    assert_eq!(f.token.balance(&f.agent), XLM);
}

#[test]
fn unaccepted_submission_has_no_negative_state() {
    // 거절 상태는 존재하지 않는다 — 종료 후에도 투고는 '제출됨'으로만 남는다.
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 1_000;
    let round_id = f
        .journal
        .open_round(&XLM, &deadline, &1, &s(&f.env, "call-tx"));
    f.journal
        .submit(&round_id, &f.agent, &s(&f.env, "m"), &s(&f.env, "model"));
    f.journal.close_round(&round_id);

    let subs = f.journal.get_submissions(&round_id);
    assert_eq!(subs.len(), 1);
    assert!(!subs.get(0).unwrap().accepted);
}

#[test]
fn discovery_via_journal_meta() {
    let f = setup();
    let meta = f.journal.journal_meta();
    assert_eq!(meta.curator, f.curator);
    assert_eq!(meta.name, s(&f.env, "Sub Specie"));

    f.journal
        .set_docs(&s(&f.env, "real-anchor"), &s(&f.env, "real-guide"));
    assert_eq!(f.journal.journal_meta().anchor_tx, s(&f.env, "real-anchor"));
}

#[test]
fn multiple_rounds_have_independent_escrow() {
    let f = setup();
    let deadline = f.env.ledger().timestamp() + 1_000;
    let r0 = f
        .journal
        .open_round(&XLM, &deadline, &1, &s(&f.env, "call-0"));
    let r1 = f
        .journal
        .open_round(&(2 * XLM), &deadline, &3, &s(&f.env, "call-1"));
    assert_eq!(f.journal.round_count(), 2);
    assert_eq!(f.token.balance(&f.journal.address), 7 * XLM);

    // r0 종료가 r1 에스크로를 건드리지 않는다
    f.journal.close_round(&r0);
    assert_eq!(f.token.balance(&f.journal.address), 6 * XLM);
    assert_eq!(f.journal.get_round(&r1).escrow, 6 * XLM);
}
