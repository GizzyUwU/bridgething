import addFormats from 'ajv-formats';
import Ajv2020, { type ValidateFunction } from 'ajv/dist/2020.js';
import schema from './schema.v1.json' with { type: 'json' };
import type { DiscoverManifest } from './types.ts';

let cachedValidator: ValidateFunction<DiscoverManifest> | null = null;

function validator(): ValidateFunction<DiscoverManifest> {
  if (cachedValidator) return cachedValidator;
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  cachedValidator = ajv.compile<DiscoverManifest>(schema);
  return cachedValidator;
}

export class ManifestValidationError extends Error {
  constructor(
    message: string,
    public readonly errors: string[],
  ) {
    super(`${message}\n  - ${errors.join('\n  - ')}`);
    this.name = 'ManifestValidationError';
  }
}

export function validateSchema(manifest: unknown): asserts manifest is DiscoverManifest {
  const v = validator();
  if (!v(manifest)) {
    const errs = (v.errors ?? []).map(e => `${e.instancePath || '<root>'}: ${e.message ?? 'invalid'}`);
    throw new ManifestValidationError('manifest failed schema validation', errs);
  }
}

export function validateInvariants(manifest: DiscoverManifest): void {
  const errors: string[] = [];

  for (const [key, release] of Object.entries(manifest.releases)) {
    if (release.version !== key) {
      errors.push(`releases["${key}"].version="${release.version}" must equal map key "${key}"`);
    }
    if (!(release.channel in manifest.channels)) {
      errors.push(
        `releases["${key}"].channel="${release.channel}" not present in channels{${Object.keys(manifest.channels).join(',')}}`,
      );
    }
  }

  const seenInChannel = new Map<string, string>();
  for (const [channelId, channel] of Object.entries(manifest.channels)) {
    if (!(channel.latest in manifest.releases)) {
      errors.push(`channels["${channelId}"].latest="${channel.latest}" not present in releases`);
    }

    const latest = manifest.releases[channel.latest];
    const installable = (version: string) => {
      const release = manifest.releases[version];
      return release !== undefined && release.yanked === null && !release.deprecated;
    };
    if (latest && !installable(channel.latest) && channel.releases.some(installable)) {
      errors.push(
        `channels["${channelId}"].latest="${channel.latest}" is withdrawn while the channel still has an installable release`,
      );
    }

    for (const version of channel.releases) {
      const release = manifest.releases[version];
      if (!release) {
        errors.push(`channels["${channelId}"].releases[] references missing release "${version}"`);
        continue;
      }
      if (release.channel !== channelId) {
        errors.push(
          `release "${version}".channel="${release.channel}" but listed in channels["${channelId}"].releases[]`,
        );
      }
      const prior = seenInChannel.get(version);
      if (prior) {
        errors.push(`release "${version}" listed in both channels["${prior}"] and channels["${channelId}"]`);
      } else {
        seenInChannel.set(version, channelId);
      }
    }
  }

  for (const version of Object.keys(manifest.releases)) {
    if (!seenInChannel.has(version)) {
      errors.push(`release "${version}" is orphaned (not listed in any channel.releases[])`);
    }
  }

  const defaults = Object.entries(manifest.channels).filter(([, c]) => c.default);
  if (defaults.length > 1) {
    errors.push(
      `at most one channel may have default=true; found ${defaults.length}: ${defaults.map(([id]) => id).join(', ')}`,
    );
  }

  if (errors.length) {
    throw new ManifestValidationError('manifest failed cross-reference invariants', errors);
  }
}

export function validate(manifest: unknown): DiscoverManifest {
  validateSchema(manifest);
  validateInvariants(manifest);
  return manifest;
}
