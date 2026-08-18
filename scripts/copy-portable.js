import { readFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url)) + '/..';
const conf = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf-8'));
const version = conf.version;
const exe = join(root, 'src-tauri/target/release/ctrip-price-tester.exe');
const destDir = join(root, 'release');
mkdirSync(destDir, { recursive: true });
const dest = join(destDir, `携程查价测试台_${version}_x64-portable.exe`);
copyFileSync(exe, dest);
console.log(`portable: ${dest}`);
