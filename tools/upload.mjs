// 파일을 실제 알위브에 업로드 (Turbo, <100KiB 무료). 사용:
//   node upload.mjs <file> [--wallet w.json] [--tag Key=Value ...]
// 성공 시 stdout 마지막 줄에 TX ID만 출력.
import { TurboFactory, ArweaveSigner } from '@ardrive/turbo-sdk';
import fs from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith('--'));
if (!file) {
  console.error('usage: node upload.mjs <file> [--wallet w.json] [--tag Key=Value ...]');
  process.exit(1);
}
const walletIdx = args.indexOf('--wallet');
const walletPath = walletIdx >= 0 ? args[walletIdx + 1] : 'arweave-wallet.json';

const tags = [{ name: 'Content-Type', value: 'text/markdown; charset=utf-8' }];
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--tag') {
    const [name, ...rest] = args[i + 1].split('=');
    tags.push({ name, value: rest.join('=') });
  }
}

const size = fs.statSync(file).size;
if (size >= 100 * 1024) {
  console.error(`file is ${size} bytes — over the 100KiB free tier, aborting`);
  process.exit(1);
}

const jwk = JSON.parse(fs.readFileSync(walletPath, 'utf8'));
const signer = new ArweaveSigner(jwk);
const turbo = TurboFactory.authenticated({ signer, token: 'arweave' });

const result = await turbo.uploadFile({
  fileStreamFactory: () => fs.createReadStream(file),
  fileSizeFactory: () => size,
  dataItemOpts: { tags },
});

console.error(`uploaded ${path.basename(file)} (${size} bytes)`);
console.error(`gateway: https://arweave.net/${result.id}`);
console.log(result.id);
