// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";
export class DataValue {
  id: string;
  name: string;
  age: number;
  favoriteColor: string;

  static fromJson(data: any): DataValue {
    const res = Object.assign(new DataValue(), data);
    return res;
  }
}



export class Data {
  key: string;
  value: DataValue;
  metadata: unknown;
  id1: string;
  id2: string;
  settings: {
    key: string;
    value: unknown;
    metadata: unknown;
  };


  static fromJson(data: any): Data {
    const res = Object.assign(new Data(), data);

    res["Data"] &&= DataValue.fromJson(res.Data);

    return res;
  }
}
export class DataScientist {
  key: string;
  value: unknown;
  metadata: unknown;
  datasets: [];


  static fromJson(data: any): DataScientist {
    const res = Object.assign(new DataScientist(), data);

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
