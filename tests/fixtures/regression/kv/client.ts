// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";
export class Scientist {
  firstname: string;
  lastname: string;
  age: number;

  static fromJson(data: any): Scientist {
    const res = Object.assign(new Scientist(), data);
    return res;
  }
}



export class Data {
  key: string;
  value: unknown;
  metadata: unknown;
  key1: string;
  key2: string;
  settings: {
    key: string;
    value: unknown;
    metadata: unknown;
  };


  static fromJson(data: any): Data {
    const res = Object.assign(new Data(), data);


    return res;
  }
}
export class DataScientist {
  key: string;
  value: Scientist;
  metadata: unknown;
  id: string;
  datasets: [];

  static async get(
    id: string,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<DataScientist>> {
    
    const baseUrl = new URL(`http://localhost:5002/api/DataScientist/get`);
    

    baseUrl.searchParams.append('id', String(id));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      DataScientist,
      false
    );
  }
  async getMetadata(
    __datasource: "withDatasets" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<unknown>> {
    const key = encodeURIComponent(String(this.key));
    const baseUrl = new URL(`http://localhost:5002/api/DataScientist/${key}/getMetadata`);
    

    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }
  static async post(
    value: DeepPartial<DataScientist>,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    
    const baseUrl = new URL(`http://localhost:5002/api/DataScientist/post`);
    const payload: any = {};

    payload["value"] = value;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }
  async putMetadata(
    metadata: unknown,
    __datasource: "withDatasets" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const key = encodeURIComponent(String(this.key));
    const baseUrl = new URL(`http://localhost:5002/api/DataScientist/${key}/putMetadata`);
    

    baseUrl.searchParams.append('metadata', String(metadata));
    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): DataScientist {
    const res = Object.assign(new DataScientist(), data);

    res["DataScientist"] &&= Scientist.fromJson(res.DataScientist);
    for (let i = 0; i < res.datasets?.length; i++) {
      res.datasets[i] = Data.fromJson(res.datasets[i]);
    }

    return res;
  }
}
export class JsonValue {
  key: string;
  value: unknown;
  metadata: unknown;


  static fromJson(data: any): JsonValue {
    const res = Object.assign(new JsonValue(), data);


    return res;
  }
}
export class StreamValue {
  key: string;
  
  metadata: unknown;


  static fromJson(data: any): StreamValue {
    const res = Object.assign(new StreamValue(), data);


    return res;
  }
}
export class TextValue {
  key: string;
  value: string;
  metadata: unknown;


  static fromJson(data: any): TextValue {
    const res = Object.assign(new TextValue(), data);


    return res;
  }
}
