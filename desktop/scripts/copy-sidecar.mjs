import { execSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../..');
const binariesDir = path.resolve(__dirname, '../src-tauri/binaries');

const debugMode = process.argv.includes('--debug');
const skipBuild = process.argv.includes('--skip-build');
const profile = debugMode ? 'debug' : 'release';
const cargoArgs = debugMode ? ['build'] : ['build', '--release'];

const ext = process.platform === 'win32' ? '.exe' : '';
const targetTriple = execSync('rustc --print host-tuple', {
  cwd: repoRoot,
  encoding: 'utf8',
}).trim();

if (!targetTriple) {
  console.error('Failed to determine Rust host target triple');
  process.exit(1);
}

const dest = path.join(binariesDir, `synbot-${targetTriple}${ext}`);

if (!skipBuild) {
  console.log(`Building synbot ${profile} binary for ${targetTriple}…`);
  const build = spawnSync('cargo', cargoArgs, {
    cwd: repoRoot,
    stdio: 'inherit',
  });
  if (build.status !== 0) {
    process.exit(build.status ?? 1);
  }
}

const source = path.join(repoRoot, 'target', profile, `synbot${ext}`);

if (!fs.existsSync(source)) {
  console.error(`Expected synbot binary not found: ${source}`);
  console.error(`Run: cargo ${cargoArgs.join(' ')} (from repo root)`);
  process.exit(1);
}

fs.mkdirSync(binariesDir, { recursive: true });
fs.copyFileSync(source, dest);
console.log(`Sidecar copied to ${dest}`);
