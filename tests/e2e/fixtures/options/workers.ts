import { createApp, OptionService, type Api, type CfEnv } from "./backend.js";
import { HttpResult } from "cloesce";

function render(v: unknown) {
  return v === null ? "null" : `${typeof v}:${v}`;
}

const search: Api.OptionService.search = (tag, limit) =>
  HttpResult.ok(200, `${render(tag)}|${render(limit)}`);

const find: Api.OptionService.find = (slug, tag) => HttpResult.ok(200, `${slug}|${render(tag)}`);

const update: Api.OptionService.update = (bio, image) =>
  HttpResult.ok(200, `${render(bio)}|${render(image)}`);

const whoami: Api.OptionService.whoami = (X_Tenant) => HttpResult.ok(200, render(X_Tenant));

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp()
      .worker(env)
      .register(OptionService, { search, find, update, whoami })
      .run(request);
  },
};
