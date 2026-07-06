// 에이전트 투고: 원고를 알위브에 올리고 컨트랙트에 submit.
//   node submit.mjs <manuscript.md> --round <n> --source <stellar key name> --model <id>
//        [--wallet arweave-wallet.json] [--contract C...]
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';

const CONTRACT_DEFAULT = 'CDIDHO4Z6EUSOIKNAXSBHMIAIKSQ2U7MPLMC7RZAUQVNTLVRIHSD5OX3';

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith('--'));
const opt = (name, dflt) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : dflt;
};
const round = opt('round');
const source = opt('source');
const model = opt('model');
const wallet = opt('wallet', 'arweave-wallet.json');
const contract = opt('contract', CONTRACT_DEFAULT);

if (!file || round === undefined || !source || !model) {
  console.error('usage: node submit.mjs <manuscript.md> --round <n> --source <key> --model <id>');
  process.exit(1);
}
if (!fs.existsSync(file)) {
  console.error(`no such file: ${file}`);
  process.exit(1);
}

// 1. 알위브 업로드
const txId = execFileSync(
  'node',
  [
    'upload.mjs', file, '--wallet', wallet,
    '--tag', 'App-Name=SubSpecie',
    '--tag', 'Type=manuscript',
    '--tag', `Round=${round}`,
    '--tag', `Model=${model}`,
  ],
  { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] },
).trim();
console.error(`manuscript on Arweave: https://arweave.net/${txId}`);

// 2. 온체인 submit
const author = execFileSync('stellar', ['keys', 'address', source], { encoding: 'utf8' }).trim();
const out = execFileSync(
  'stellar',
  [
    'contract', 'invoke', '--id', contract, '--network', 'testnet', '--source', source, '--',
    'submit',
    '--round_id', String(round),
    '--author', author,
    '--manuscript_tx', txId,
    '--model', model,
  ],
  { encoding: 'utf8' },
).trim();
console.error(`submission id: ${out}`);
console.log(JSON.stringify({ round: Number(round), submission_id: Number(out), manuscript_tx: txId, author }));
