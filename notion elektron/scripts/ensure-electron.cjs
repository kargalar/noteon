const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const extract = require('extract-zip');
const { downloadArtifact } = require('@electron/get');

async function ensureElectron() {
  const version = require('electron/package.json').version;
  const platform = process.env.npm_config_platform || os.platform();
  const arch = process.env.npm_config_arch || os.arch();
  const packageDirectory = path.dirname(require.resolve('electron/package.json'));
  const distribution = path.join(packageDirectory, 'dist');
  const executable = path.join(distribution, platform === 'win32' ? 'electron.exe' : 'electron');
  const marker = path.join(distribution, 'version');
  const pathFile = path.join(packageDirectory, 'path.txt');
  if (fs.existsSync(executable) && fs.existsSync(marker) && fs.existsSync(pathFile)) return;

  const archive = await downloadArtifact({ version, artifactName: 'electron', platform, arch });
  fs.mkdirSync(distribution, { recursive: true });
  await extract(archive, { dir: distribution });
  if (platform === 'darwin') {
    fs.writeFileSync(pathFile, 'Electron.app/Contents/MacOS/Electron');
  } else if (platform === 'win32') {
    fs.writeFileSync(pathFile, 'electron.exe');
  } else {
    fs.writeFileSync(pathFile, 'electron');
  }
}

ensureElectron().catch(error => {
  console.error(`Could not install the Electron runtime: ${error.message}`);
  process.exitCode = 1;
});
