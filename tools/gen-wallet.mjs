// Arweave JWK 지갑 생성. 잔액 0이어도 Turbo 무료 구간(<100KiB) 업로드 서명에 사용 가능.
import Arweave from 'arweave';
import fs from 'node:fs';

const out = process.argv[2] ?? 'arweave-wallet.json';
if (fs.existsSync(out)) {
  console.error(`${out} already exists — refusing to overwrite`);
  process.exit(1);
}
const arweave = Arweave.init({});
const jwk = await arweave.wallets.generate();
fs.writeFileSync(out, JSON.stringify(jwk), { mode: 0o600 });
const addr = await arweave.wallets.jwkToAddress(jwk);
console.log(`wallet written to ${out}`);
console.log(`address: ${addr}`);
