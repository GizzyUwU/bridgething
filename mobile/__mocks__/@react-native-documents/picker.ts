type Picked = { uri: string; name: string | null };

type Copied = { uri: string; fileName: string; destination: string };

export type FakePicker = {
  picked: Picked | null;
  copyError: string | null;
  pickOptions: unknown[];
  copies: Copied[];
};

const world: FakePicker = {
  picked: { uri: 'file:///var/inbox/my-webapp.zip', name: 'my-webapp.zip' },
  copyError: null,
  pickOptions: [],
  copies: [],
};

export function fakePicker(): FakePicker {
  return world;
}

export const errorCodes = Object.freeze({
  OPERATION_CANCELED: 'OPERATION_CANCELED',
  IN_PROGRESS: 'ASYNC_OP_IN_PROGRESS',
  UNABLE_TO_OPEN_FILE_TYPE: 'UNABLE_TO_OPEN_FILE_TYPE',
  NULL_PRESENTER: 'NULL_PRESENTER',
});

export const types = Object.freeze({ zip: 'public.zip-archive' });

export function isErrorWithCode(error: unknown): boolean {
  return typeof (error as { code?: unknown } | null)?.code === 'string';
}

export async function pick(options: unknown): Promise<Picked[]> {
  world.pickOptions.push(options);
  if (!world.picked) {
    throw Object.assign(new Error('user canceled document picker'), {
      code: errorCodes.OPERATION_CANCELED,
    });
  }
  return [world.picked];
}

export async function keepLocalCopy(options: {
  files: { uri: string; fileName: string }[];
  destination: string;
}): Promise<
  (
    | { status: 'success'; sourceUri: string; localUri: string }
    | { status: 'error'; sourceUri: string; copyError: string }
  )[]
> {
  return options.files.map(file => {
    world.copies.push({ ...file, destination: options.destination });
    if (world.copyError) {
      return {
        status: 'error' as const,
        sourceUri: file.uri,
        copyError: world.copyError,
      };
    }
    return {
      status: 'success' as const,
      sourceUri: file.uri,
      localUri: `file:///caches/${file.fileName}`,
    };
  });
}
