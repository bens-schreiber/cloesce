declare module "*.jsonc" {
  const value: {
    workers_url?: string;
    [key: string]: unknown;
  };
  export default value;
}
