import { execFileSync, execSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

// Install lefthook hooks during prepare. A failed install MUST fail the whole
// install: swallowing the error leaves commit-msg missing and the gate silently dead.

function git(args, options = {}) {
  return execFileSync('git', args, { encoding: 'utf8', ...options });
}

let inWorkTree = true;
try {
  execFileSync('git', ['rev-parse', '--git-dir'], { stdio: 'ignore' });
} catch {
  inWorkTree = false;
}

if (!inWorkTree) {
  // Tarballs / CI checkouts without .git: skipping is expected, not a failure.
  console.log('[lefthook] Not a git work tree, skipping hook installation');
  process.exit(0);
}

let hooksPath = '';
try {
  hooksPath = git(['config', '--get', 'core.hooksPath']).trim();
} catch {
  // core.hooksPath is unset; use git's default hooks directory.
}

// lefthook refuses custom core.hooksPath by default; --force targets that directory.
const installArgs = hooksPath ? ['install', '--force'] : ['install'];

try {
  // Shell is required to resolve pnpm's node_modules/.bin/lefthook.cmd on Windows.
  execSync(`lefthook ${installArgs.join(' ')}`, { stdio: 'inherit' });
} catch (error) {
  const forceSuffix = hooksPath ? ' --force' : '';
  console.error(
    `[lefthook] Hook installation failed (core.hooksPath=${hooksPath || 'default'}).\n` +
      `  Run manually: pnpm exec lefthook install${forceSuffix}\n` +
      '  Common causes: hooksPath points to a missing directory, directory is read-only, or git is too old.'
  );
  process.exit(error.status || 1);
}

// Verify commit-msg actually landed where git will look for it.
const repoRoot = git(['rev-parse', '--show-toplevel']).trim();
const defaultHooks = git(['rev-parse', '--git-path', 'hooks']).trim();
const hooksDir = hooksPath
  ? path.resolve(repoRoot, hooksPath)
  : path.resolve(repoRoot, defaultHooks);
const commitMsgHook = path.join(hooksDir, 'commit-msg');

if (!existsSync(commitMsgHook)) {
  console.error(
    `[lefthook] commit-msg hook not found in ${hooksDir} after install; the commit gate would not run.`
  );
  process.exit(1);
}

console.log(`[lefthook] Commit gate ready: ${commitMsgHook}`);