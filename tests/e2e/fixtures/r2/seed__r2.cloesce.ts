import {
  R2,
  WranglerEnv,
  KeyParam,
  Model,
  Integer,
  DataSource,
  Inject,
  Put,
} from "cloesce/backend";
import {
  D1Database,
  R2ObjectBody,
  R2Bucket,
  ReadableStream,
} from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
  bucket1: R2Bucket;
  bucket2: R2Bucket;
}

@Model(["GET"])
export class PureR2Model {
  @KeyParam
  id: string;

  @R2("path/to/data/{id}", "bucket1")
  data: R2ObjectBody;

  @R2("path/to/other/{id}", "bucket2")
  otherData: R2ObjectBody;

  @R2("path/", "bucket1")
  allData: R2ObjectBody[];

  @Put()
  async uploadData(@Inject env: Env, stream: ReadableStream) {
    await env.bucket1.put(`path/to/data/${this.id}`, stream);
  }

  @Put()
  async uploadOtherData(@Inject env: Env, stream: ReadableStream) {
    await env.bucket2.put(`path/to/other/${this.id}`, stream);
  }
}

@Model(["GET", "SAVE", "LIST"])
export class D1BackedModel {
  id: Integer;

  @KeyParam
  keyParam: string;

  someColumn: number;
  someOtherColumn: string;

  @R2("d1Backed/{id}/{keyParam}/{someColumn}/{someOtherColumn}", "bucket1")
  r2Data: R2ObjectBody;

  @Put()
  async uploadData(@Inject env: Env, stream: ReadableStream) {
    await env.bucket1.put(
      `d1Backed/${this.id}/${this.keyParam}/${this.someColumn}/${this.someOtherColumn}`,
      stream,
    );
  }
}
