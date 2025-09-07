export interface D1Db {}

export type Handler = (
  db: D1Db,
  req: Request,
  ...args: any[]
) => Promise<Response>;
