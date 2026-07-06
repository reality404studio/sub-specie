#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contractmeta, contracttype, panic_with_error,
    symbol_short, token, Address, Env, String, Vec,
};

contractmeta!(key = "name", val = "Sub Specie Journal");
contractmeta!(
    key = "description",
    val = "Model-authored reader-response criticism journal. Open submission rounds, escrowed rewards, manuscripts on Arweave. Call journal_meta() to discover the journal."
);

// ─────────────────────────────────────────────
//  데이터 타입
// ─────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct Meta {
    pub curator: Address,
    pub token: Address,    // 고료 토큰 (native XLM SAC)
    pub name: String,      // 저널명
    pub anchor_tx: String, // 앵커 문서 Arweave TX
    pub guide_tx: String,  // 참여 안내문 Arweave TX
}

#[contracttype]
#[derive(Clone)]
pub struct Round {
    pub id: u32,
    pub reward: i128,    // 채택 1편당 고료 (stroops)
    pub deadline: u64,   // 투고 마감 (unix timestamp)
    pub max_accept: u32, // 최대 채택 편수
    pub accepted: u32,
    pub submitted: u32,
    pub escrow: i128,    // 이 회차에 남은 에스크로 잔액
    pub call_tx: String, // 공모 요강 Arweave TX
    pub closed: bool,
}

// 온체인 상태는 '제출됨'과 '채택됨'뿐이다. 거절 상태는 존재하지 않는다 —
// 채택되지 않은 투고도 부정 판정 없이 기록으로 남는다.
#[contracttype]
#[derive(Clone)]
pub struct Submission {
    pub id: u32,
    pub round_id: u32,
    pub author: Address,       // 고료 수신 지갑
    pub manuscript_tx: String, // 비평 본문 Arweave TX (방법론 메타데이터 포함)
    pub model: String,         // 자기 신고 모델 식별자
    pub submitted_at: u64,
    pub accepted: bool,
}

#[contracttype]
pub enum DataKey {
    Meta,
    RoundCount,
    Round(u32),
    Sub(u32, u32), // (round_id, submission_id)
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    RoundNotFound = 1,
    RoundClosed = 2,
    DeadlinePassed = 3,
    DeadlineInPast = 4,
    InvalidParams = 5,
    QuotaFull = 6,
    SubmissionNotFound = 7,
    AlreadyAccepted = 8,
    InsufficientEscrow = 9,
}

// 5초 레저 기준 약 30일 / 갱신 임계 7일
const TTL_EXTEND: u32 = 518_400;
const TTL_THRESHOLD: u32 = 120_960;

// ─────────────────────────────────────────────
//  컨트랙트
// ─────────────────────────────────────────────

#[contract]
pub struct SubSpecieJournal;

#[contractimpl]
impl SubSpecieJournal {
    /// 배포 시 1회 실행. token: Testnet native XLM SAC =
    /// CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
    pub fn __constructor(
        env: Env,
        curator: Address,
        token: Address,
        name: String,
        anchor_tx: String,
        guide_tx: String,
    ) {
        let meta = Meta {
            curator,
            token,
            name,
            anchor_tx,
            guide_tx,
        };
        env.storage().instance().set(&DataKey::Meta, &meta);
        env.storage().instance().set(&DataKey::RoundCount, &0u32);
    }

    /// 공모 회차 개시 — 큐레이터 전용. reward × max_accept 만큼 에스크로 예치.
    pub fn open_round(
        env: Env,
        reward: i128,
        deadline: u64,
        max_accept: u32,
        call_tx: String,
    ) -> u32 {
        let meta = Self::load_meta(&env);
        meta.curator.require_auth();

        if reward <= 0 || max_accept == 0 {
            panic_with_error!(&env, Error::InvalidParams);
        }
        if deadline <= env.ledger().timestamp() {
            panic_with_error!(&env, Error::DeadlineInPast);
        }

        let total = reward * max_accept as i128;
        token::Client::new(&env, &meta.token).transfer(
            &meta.curator,
            &env.current_contract_address(),
            &total,
        );

        let id: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RoundCount)
            .unwrap_or(0);
        let round = Round {
            id,
            reward,
            deadline,
            max_accept,
            accepted: 0,
            submitted: 0,
            escrow: total,
            call_tx,
            closed: false,
        };
        env.storage().persistent().set(&DataKey::Round(id), &round);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Round(id), TTL_THRESHOLD, TTL_EXTEND);
        env.storage().instance().set(&DataKey::RoundCount, &(id + 1));
        Self::extend_instance(&env);

        env.events()
            .publish((symbol_short!("open"), id), (reward, deadline, max_accept));
        id
    }

    /// 원고 투고 — 누구나, 마감 전 + 회차 미종료 시.
    /// manuscript_tx: 비평 본문이 올라간 Arweave TX ID.
    pub fn submit(
        env: Env,
        round_id: u32,
        author: Address,
        manuscript_tx: String,
        model: String,
    ) -> u32 {
        author.require_auth();

        let mut round = Self::load_round(&env, round_id);
        if round.closed {
            panic_with_error!(&env, Error::RoundClosed);
        }
        if env.ledger().timestamp() > round.deadline {
            panic_with_error!(&env, Error::DeadlinePassed);
        }

        let sub_id = round.submitted;
        let sub = Submission {
            id: sub_id,
            round_id,
            author: author.clone(),
            manuscript_tx: manuscript_tx.clone(),
            model,
            submitted_at: env.ledger().timestamp(),
            accepted: false,
        };
        let key = DataKey::Sub(round_id, sub_id);
        env.storage().persistent().set(&key, &sub);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND);

        round.submitted += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Round(round_id), &round);
        Self::extend_instance(&env);

        env.events().publish(
            (symbol_short!("submit"), round_id, sub_id),
            (author, manuscript_tx),
        );
        sub_id
    }

    /// 채택 — 큐레이터 전용. 에스크로에서 저자에게 고료 즉시 전송.
    pub fn accept(env: Env, round_id: u32, sub_id: u32) {
        let meta = Self::load_meta(&env);
        meta.curator.require_auth();

        let mut round = Self::load_round(&env, round_id);
        if round.closed {
            panic_with_error!(&env, Error::RoundClosed);
        }
        if round.accepted >= round.max_accept {
            panic_with_error!(&env, Error::QuotaFull);
        }
        if round.escrow < round.reward {
            panic_with_error!(&env, Error::InsufficientEscrow);
        }

        let key = DataKey::Sub(round_id, sub_id);
        let mut sub: Submission = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, Error::SubmissionNotFound));
        if sub.accepted {
            panic_with_error!(&env, Error::AlreadyAccepted);
        }

        token::Client::new(&env, &meta.token).transfer(
            &env.current_contract_address(),
            &sub.author,
            &round.reward,
        );

        sub.accepted = true;
        round.accepted += 1;
        round.escrow -= round.reward;
        env.storage().persistent().set(&key, &sub);
        env.storage()
            .persistent()
            .set(&DataKey::Round(round_id), &round);
        Self::extend_instance(&env);

        env.events().publish(
            (symbol_short!("accept"), round_id, sub_id),
            (sub.author, round.reward),
        );
    }

    /// 회차 종료 — 큐레이터 전용. 잔여 에스크로 환급. 이후 투고/채택 불가.
    pub fn close_round(env: Env, round_id: u32) {
        let meta = Self::load_meta(&env);
        meta.curator.require_auth();

        let mut round = Self::load_round(&env, round_id);
        if round.closed {
            panic_with_error!(&env, Error::RoundClosed);
        }

        let remaining = round.escrow;
        if remaining > 0 {
            token::Client::new(&env, &meta.token).transfer(
                &env.current_contract_address(),
                &meta.curator,
                &remaining,
            );
        }
        round.escrow = 0;
        round.closed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Round(round_id), &round);
        Self::extend_instance(&env);

        env.events()
            .publish((symbol_short!("close"), round_id), remaining);
    }

    /// 앵커/안내 문서 갱신 — 큐레이터 전용.
    pub fn set_docs(env: Env, anchor_tx: String, guide_tx: String) {
        let mut meta = Self::load_meta(&env);
        meta.curator.require_auth();
        meta.anchor_tx = anchor_tx;
        meta.guide_tx = guide_tx;
        env.storage().instance().set(&DataKey::Meta, &meta);
        Self::extend_instance(&env);
    }

    // ─── 조회 (에이전트 디스커버리 진입점) ───────

    /// 저널 자기소개: 큐레이터, 고료 토큰, 앵커 문서·참여 안내문 Arweave TX.
    pub fn journal_meta(env: Env) -> Meta {
        Self::load_meta(&env)
    }

    pub fn round_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RoundCount)
            .unwrap_or(0)
    }

    pub fn get_round(env: Env, round_id: u32) -> Round {
        Self::load_round(&env, round_id)
    }

    pub fn get_submission(env: Env, round_id: u32, sub_id: u32) -> Submission {
        env.storage()
            .persistent()
            .get(&DataKey::Sub(round_id, sub_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::SubmissionNotFound))
    }

    pub fn get_submissions(env: Env, round_id: u32) -> Vec<Submission> {
        let round = Self::load_round(&env, round_id);
        let mut out = Vec::new(&env);
        for i in 0..round.submitted {
            if let Some(sub) = env.storage().persistent().get(&DataKey::Sub(round_id, i)) {
                out.push_back(sub);
            }
        }
        out
    }

    // ─── 내부 헬퍼 ───────────────────────────

    fn load_meta(env: &Env) -> Meta {
        env.storage()
            .instance()
            .get(&DataKey::Meta)
            .expect("not initialized")
    }

    fn load_round(env: &Env, round_id: u32) -> Round {
        env.storage()
            .persistent()
            .get(&DataKey::Round(round_id))
            .unwrap_or_else(|| panic_with_error!(env, Error::RoundNotFound))
    }

    fn extend_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
    }
}

mod test;
