import { validate } from '../../sdk/sources.ts';
import surfacesJson from '../../sdk/surfaces.json';
import type { SurfaceDocs } from '../../sdk/types.ts';

let cached: SurfaceDocs | null = null;

export async function loadSurfaceDocs(): Promise<SurfaceDocs> {
  if (!cached) cached = validate(surfacesJson as unknown);
  return cached;
}
