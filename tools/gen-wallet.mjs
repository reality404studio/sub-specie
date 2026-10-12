// Arweave JWK 지갑 생성. 잔액 0이어도 Turbo 무료 구간(<100KiB) 업로드 서명에 사용 가능.
import Arweave from 'arweave';
import fs from 'node:fs';
import path from 'node:path';

const out = process.argv[2] ?? 'arweave-wallet.json';
const resolved = path.resolve(out);

// 저장소 안에 쓰면 커밋될 수 있다. 기본 동작(파일 생성)은 그대로 두고 경고만 한다.
function findRepoRoot(start) {
  let dir = path.resolve(start);
  for (;;) {
    if (fs.existsSync(path.join(dir, '.git'))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}
const repoRoot = findRepoRoot(path.dirname(resolved));
if (repoRoot) {
  console.warn(
    `warning: ${resolved} is inside the git repository (${repoRoot}). ` +
      'This file is a secret. It is gitignored under the default name, but do not commit it. ' +
      'Prefer a path outside the repository.',
  );
}

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
