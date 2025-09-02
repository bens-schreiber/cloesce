export interface D1Db {
  prepare(sql: string): {
    bind(...args: unknown[]): any;
    run(): Promise<{ success: boolean }>;
    first<T = unknown>(): Promise<T | undefined>;
    all<T = unknown>(): Promise<{ results: T[] }>;
  };
}

export type Handler = (db: D1Db, req: Request, ...args: any[]) => Promise<Response>;
