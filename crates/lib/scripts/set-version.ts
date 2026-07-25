const WORKSPACE_MANIFEST = '../../Cargo.toml';

const cargoToml: string = await Bun.file(WORKSPACE_MANIFEST).text();
const workspacePackage: string | undefined = cargoToml
  .split(/^\[/m)
  .find(section => section.startsWith('workspace.package]'));

if (!workspacePackage) {
  console.error(`[workspace.package] not found in ${WORKSPACE_MANIFEST}`);
  process.exit(1);
}

const version: string | undefined = workspacePackage.match(/^\s*version\s*=\s*["']([^"']+)["']/m)?.[1];

if (!version) {
  console.error(`version not found in [workspace.package] of ${WORKSPACE_MANIFEST}`);
  process.exit(1);
}

const file = Bun.file('./ts/index.ts');
const index: string = await file.text();

const regex: RegExp = /(export\s+const\s+LIBBRIDGETHING_VERSION\s*=\s*['"`])[^'"`]+(['"`])/;
if (!regex.test(index)) {
  console.error('LIBBRIDGETHING_VERSION not found in ./ts/index.ts');
  process.exit(1);
}

const newIndex = index.replace(regex, `$1v${version}$2`);
await file.write(newIndex);

console.log(`Updated LIBBRIDGETHING_VERSION to v${version}`);
