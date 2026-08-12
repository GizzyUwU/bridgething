export type ComponentNotes = {
  version: string;
  summary: string;
  body: string;
};

export type ComposeInput = {
  daemon: ComponentNotes;
  image: ComponentNotes;
  daemonBumped: boolean;
  imageBumped: boolean;
};

export type ComposeOutput = {
  changelog: string;
  summary: string;
};

const NO_CHANGE = '_no change since previous release._';

function trimBody(body: string): string {
  return body.replace(/^\s+|\s+$/g, '');
}

export function compose(input: ComposeInput): ComposeOutput {
  if (!input.daemonBumped && !input.imageBumped) {
    throw new Error("compose() called with neither component bumped; that isn't a release");
  }

  const daemonBody = input.daemonBumped ? trimBody(input.daemon.body) : NO_CHANGE;
  const imageBody = input.imageBumped ? trimBody(input.image.body) : NO_CHANGE;

  const changelog =
    `## daemon ${input.daemon.version}\n\n${daemonBody}\n\n` + `## image ${input.image.version}\n\n${imageBody}\n`;

  const summary = input.daemonBumped ? input.daemon.summary : input.image.summary;

  return { changelog, summary };
}

export function composeVersion(daemonVersion: string, imageVersion: string): string {
  return `${daemonVersion}+image.${imageVersion}`;
}

export function parseVersion(composite: string): {
  daemon: string;
  image: string;
} {
  const match = /^([A-Za-z0-9.\-]+)\+image\.([A-Za-z0-9.\-]+)$/.exec(composite);
  if (!match) {
    throw new Error(`version "${composite}" doesn't match composite shape "<daemon>+image.<image>"`);
  }
  return { daemon: match[1]!, image: match[2]! };
}
