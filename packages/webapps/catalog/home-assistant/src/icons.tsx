import { domainOf } from './domains';

const PATHS: Record<string, React.ReactNode> = {
  light: (
    <>
      <path d="M9 18h6M10 22h4" />
      <path d="M12 2a7 7 0 0 0-4 12.7V18h8v-3.3A7 7 0 0 0 12 2Z" />
    </>
  ),
  switch: (
    <>
      <path d="M12 2v10" />
      <path d="M18.4 6.6a9 9 0 1 1-12.8 0" />
    </>
  ),
  fan: (
    <>
      <circle cx="12" cy="12" r="2" />
      <path d="M12 10V4a4 4 0 0 1 0 8M14 12h6a4 4 0 0 1-8 0M12 14v6a4 4 0 0 1 0-8M10 12H4a4 4 0 0 1 8 0" />
    </>
  ),
  climate: (
    <>
      <path d="M14 14.8V4a2 2 0 1 0-4 0v10.8a4 4 0 1 0 4 0Z" />
      <circle cx="12" cy="18" r="1.5" fill="currentColor" stroke="none" />
    </>
  ),
  scene: (
    <>
      <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1" />
      <circle cx="12" cy="12" r="3.2" />
    </>
  ),
  script: (
    <>
      <path d="M5 3h10l4 4v14H5z" />
      <path d="M15 3v4h4M9 12h6M9 16h6" />
    </>
  ),
  automation: <path d="M13 2 4 14h6l-1 8 9-12h-6z" />,
  lock: (
    <>
      <rect x="4" y="10" width="16" height="11" rx="2" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </>
  ),
  cover: (
    <>
      <rect x="3" y="3" width="18" height="18" rx="1.5" />
      <path d="M3 8h18M3 13h18M3 18h18" />
    </>
  ),
  binary_sensor: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor" stroke="none" />
    </>
  ),
  input_boolean: (
    <>
      <rect x="2" y="7" width="20" height="10" rx="5" />
      <circle cx="16" cy="12" r="2.6" fill="currentColor" stroke="none" />
    </>
  ),
  button: (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="3.2" fill="currentColor" stroke="none" />
    </>
  ),
  sensor: (
    <>
      <path d="M4 20V10M10 20V4M16 20v-7M22 20H2" />
    </>
  ),
};

export function DomainIcon({ entityId, size = 26 }: { entityId: string; size?: number }) {
  const domain = domainOf(entityId);
  const key = domain === 'input_button' ? 'button' : domain;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true">
      {PATHS[key] ?? PATHS['sensor']}
    </svg>
  );
}
