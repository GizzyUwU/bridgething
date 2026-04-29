const cargoToml: string = await Bun.file('./Cargo.toml').text();
const version: string | undefined = cargoToml.match(/version\s*=\s*["']([^"']+)["']/)?.[1];

if (!version) {
  console.error('Version not found in Cargo.toml');
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
