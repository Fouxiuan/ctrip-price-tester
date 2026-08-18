#!/usr/bin/env node
import { createWriteStream, existsSync, mkdirSync, readdirSync, renameSync, rmSync } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectDir = path.resolve(scriptDir, '..');
const resourceDir = path.join(projectDir, 'src-tauri', 'resources', 'node');
const cacheDir = path.join(projectDir, '.cache');
const nodeVersion = 'v22.14.0';
const opencliVersion = '1.8.6';
const opencliRuntimeDir = path.join(resourceDir, 'opencli-runtime');
const archiveName = `node-${nodeVersion}-win-x64.zip`;
const archivePath = path.join(cacheDir, archiveName);
const downloadUrl = `https://nodejs.org/dist/${nodeVersion}/${archiveName}`;

function log(message) { console.log(`[runtime] ${message}`); }

async function download(url, destination) {
  const response = await fetch(url, { redirect: 'follow' });
  if (!response.ok || !response.body) throw new Error(`下载 Node.js 失败：HTTP ${response.status}`);
  await pipeline(response.body, createWriteStream(destination));
}

function findOpencli() {
  return [
    path.join(opencliRuntimeDir, 'node_modules', '@jackwener', 'opencli', 'dist', 'src', 'main.js'),
    path.join(resourceDir, 'node_modules', '@jackwener', 'opencli', 'dist', 'src', 'main.js'),
  ].find(existsSync);
}

async function main() {
  if (process.platform !== 'win32') throw new Error('当前独立测试应用只打包 Windows 版本。');
  mkdirSync(resourceDir, { recursive: true });
  mkdirSync(cacheDir, { recursive: true });
  const nodeExe = path.join(resourceDir, 'node.exe');

  if (!existsSync(nodeExe)) {
    if (!existsSync(archivePath)) {
      log(`下载便携 Node.js ${nodeVersion}…`);
      await download(downloadUrl, archivePath);
    }
    log('解压便携 Node.js…');
    const extractedRoot = path.join(resourceDir, `node-${nodeVersion}-win-x64`);
    execFileSync('powershell.exe', ['-NoProfile', '-Command', 'Expand-Archive', '-LiteralPath', archivePath, '-DestinationPath', resourceDir, '-Force'], { stdio: 'inherit' });
    for (const name of readdirSync(extractedRoot)) {
      const destination = path.join(resourceDir, name);
      if (existsSync(destination)) rmSync(destination, { recursive: true, force: true });
      renameSync(path.join(extractedRoot, name), destination);
    }
    rmSync(extractedRoot, { recursive: true, force: true });
  }

  const npmCli = path.join(resourceDir, 'node_modules', 'npm', 'bin', 'npm-cli.js');
  const opencli = findOpencli();
  let needsInstall = !opencli;
  if (opencli) {
    try {
      const version = execFileSync(nodeExe, [opencli, '--version'], { encoding: 'utf8', env: { ...process.env, PATH: `${resourceDir};${process.env.PATH}` } });
      needsInstall = !version.includes(opencliVersion);
    } catch { needsInstall = true; }
  }

  if (needsInstall) {
    log(`安装 @jackwener/opencli@${opencliVersion}…`);
    const npmCache = path.join(cacheDir, 'npm');
    mkdirSync(npmCache, { recursive: true });
    mkdirSync(opencliRuntimeDir, { recursive: true });
    execFileSync(nodeExe, [npmCli, 'install', '--prefix', opencliRuntimeDir, '--omit=dev', `@jackwener/opencli@${opencliVersion}`], {
      cwd: resourceDir,
      stdio: 'inherit',
      env: { ...process.env, PATH: `${resourceDir};${process.env.PATH}`, npm_config_cache: npmCache },
    });
  }

  const installed = findOpencli();
  if (!installed) throw new Error('OpenCLI 安装完成后仍未找到命令入口。');
  const version = execFileSync(nodeExe, [installed, '--version'], { encoding: 'utf8', env: { ...process.env, PATH: `${resourceDir};${process.env.PATH}` } }).trim();
  log(`运行时准备完成：${version}`);
}

main().catch(error => { console.error(`[runtime] ${error.message}`); process.exit(1); });
