const LogVerbosity = {
  Trace: 5,
  Debug: 4,
  Log: 3,
  Warn: 2,
  Error: 1,
  Silent: 0,
} as const;
type LogVerbosity = (typeof LogVerbosity)[keyof typeof LogVerbosity];

class Logger {
  constructor(
    private readonly name: string,
    private readonly logLevel: LogVerbosity,
  ) {}

  trace = (...data: unknown[]) =>
    this.logLevel >= LogVerbosity.Trace ? console.log(`${this.name} TRACE >>`, ...data) : null;
  debug = (...data: unknown[]) =>
    this.logLevel >= LogVerbosity.Debug ? console.log(`${this.name} DEBUG >>`, ...data) : null;
  log = (...data: unknown[]) =>
    this.logLevel >= LogVerbosity.Log ? console.log(`${this.name} LOG   >>`, ...data) : null;
  warn = (...data: unknown[]) =>
    this.logLevel >= LogVerbosity.Warn ? console.warn(`${this.name} WARN  >>`, ...data) : null;
  error = (...data: unknown[]) =>
    this.logLevel >= LogVerbosity.Error ? console.error(`${this.name} ERROR >>`, ...data) : null;
}

export { LogVerbosity, Logger };
