export type WranglerConfigFormat = "toml" | "jsonc";

export interface CloesceConfigOptions {
  /**
   * Source paths containing .cloesce.ts files
   */
  srcPaths: string[];

  /**
   * Output directory for generated files (default: .generated)
   */
  outPath?: string;

  /**
   * Workers URL for API endpoints
   */
  workersUrl?: string;

  /**
   * Path to migrations directory (default: ./migrations)
   */
  migrationsPath?: string;

  /**
   * Wrangler config format used by compile/migrate (default: toml)
   */
  wranglerConfigFormat?: WranglerConfigFormat;

  /**
   * Whether to truncate source paths to just the filename
   */
  truncateSourcePaths?: boolean;
}

/** @internal */
export type DefaultCloesceConfig = Required<CloesceConfigOptions>;

export function defaultConfig(
  config: CloesceConfigOptions,
): DefaultCloesceConfig {
  return {
    srcPaths: config.srcPaths,
    outPath: config.outPath ?? ".cloesce",
    workersUrl: config.workersUrl ?? "http://localhost:8787",
    migrationsPath: config.migrationsPath ?? "./migrations",
    wranglerConfigFormat: config.wranglerConfigFormat ?? "toml",
    truncateSourcePaths: config.truncateSourcePaths ?? false,
  };
}
