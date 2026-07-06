// 큐레이터 심사 도구.
//   node review.mjs list <round>             — 투고 목록 (채택 여부 포함)
//   node review.mjs read <round> <sub_id>    — 원고 본문 출력 (알위브 게이트웨이)
//   node review.mjs accept <round> <sub_id>  — 채택 → 에스크로에서 고료 자동 전송
//   node review.mjs close <round>            — 회차 종료, 잔여 에스크로 환급
//   [--source curator] [--contract C...]
import { execFileSync } from 'node:child_process';

const CONTRACT_DEFAULT = 'CDIDHO4Z6EUSOIKNAXSBHMIAIKSQ2U7MPLMC7RZAUQVNTLVRIHSD5OX3';

const args = process.argv.slice(2);
const opt = (name, dflt) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : dflt;
};
const source = opt('source', 'curator');
const contract = opt('contract', CONTRACT_DEFAULT);
const positional = args.filter((a, i) => !a.startsWith('--') && !(args[i - 1] ?? '').startsWith('--'));
const [cmd, roundArg, subArg] = positional;

const invoke = (fn, fnArgs = []) =>
  execFileSync(
    'stellar',
    [
      'contract', 'invoke', '--id', contract, '--network', 'testnet', '--source', source,
      '--', fn, ...fnArgs,
    ],
    { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
  ).trim();

const usage = () => {
  console.error('usage: node review.mjs <list|read|accept|close> <round> [sub_id]');
  process.exit(1);
};
if (!cmd || roundArg === undefined) usage();

if (cmd === 'list') {
  const round = JSON.parse(invoke('get_round', ['--round_id', roundArg]));
  const rewardXlm = Number(BigInt(round.reward)) / 1e7;
  const escrowXlm = Number(BigInt(round.escrow)) / 1e7;
  console.log(
    `round ${round.id} — reward ${rewardXlm} XLM/편, 채택 ${round.accepted}/${round.max_accept}, ` +
      `에스크로 ${escrowXlm} XLM, 마감 ${new Date(round.deadline * 1000).toISOString()}, ` +
      `${round.closed ? '종료됨' : '진행중'}`,
  );
  const subs = JSON.parse(invoke('get_submissions', ['--round_id', roundArg]));
  if (subs.length === 0) console.log('투고 없음');
  for (const s of subs) {
    console.log(
      `#${s.id} ${s.accepted ? '[채택]' : '[제출]'} model=${s.model} ` +
        `author=${s.author.slice(0, 8)}… https://arweave.net/${s.manuscript_tx}`,
    );
  }
} else if (cmd === 'read') {
  if (subArg === undefined) usage();
  const s = JSON.parse(invoke('get_submission', ['--round_id', roundArg, '--sub_id', subArg]));
  // 새 업로드는 arweave.net 인덱싱이 늦을 수 있어 AR.IO 게이트웨이 순차 폴백
  const gateways = ['https://arweave.net', 'https://permagate.io', 'https://g8way.io'];
  let body = null;
  for (const gw of gateways) {
    try {
      const res = await fetch(`${gw}/${s.manuscript_tx}`);
      if (res.ok) {
        body = await res.text();
        console.error(`(via ${gw})`);
        break;
      }
    } catch {}
  }
  if (body === null) {
    console.error(`no gateway has TX ${s.manuscript_tx} yet (전파 대기 중일 수 있음)`);
    process.exit(1);
  }
  console.log(body);
} else if (cmd === 'accept') {
  if (subArg === undefined) usage();
  invoke('accept', ['--round_id', roundArg, '--sub_id', subArg]);
  const s = JSON.parse(invoke('get_submission', ['--round_id', roundArg, '--sub_id', subArg]));
  console.log(`채택 완료 — #${s.id} → ${s.author} 고료 전송됨`);
} else if (cmd === 'close') {
  invoke('close_round', ['--round_id', roundArg]);
  console.log(`round ${roundArg} 종료, 잔여 에스크로 환급됨`);
} else {
  usage();
}
