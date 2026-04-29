const LogLevel = {
  Trace: 5,
  Debug: 4,
  Log: 3,
  Warn: 2,
  Error: 1,
  Silent: 0,
} as const;
type LogLevel = (typeof LogLevel)[keyof typeof LogLevel];

class Logger {
  constructor(
    private readonly name: string,
    private readonly logLevel: LogLevel,
  ) {}

  trace = (...data: unknown[]) =>
    this.logLevel >= LogLevel.Trace ? console.log(`${this.name} TRACE >>`, ...data) : null;
  debug = (...data: unknown[]) =>
    this.logLevel >= LogLevel.Debug ? console.log(`${this.name} DEBUG >>`, ...data) : null;
  log = (...data: unknown[]) => (this.logLevel >= LogLevel.Log ? console.log(`${this.name} LOG   >>`, ...data) : null);
  warn = (...data: unknown[]) =>
    this.logLevel >= LogLevel.Warn ? console.warn(`${this.name} WARN  >>`, ...data) : null;
  error = (...data: unknown[]) =>
    this.logLevel >= LogLevel.Error ? console.error(`${this.name} ERROR >>`, ...data) : null;
}

export { LogLevel, Logger };
