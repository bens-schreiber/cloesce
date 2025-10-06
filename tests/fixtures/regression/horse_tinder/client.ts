import { HttpResult, instantiateObjectArray } from "cloesce";


export class Horse {
  id: number;
  name: string;
  bio: string | null;
  likes: Like[];

  static async get(
        id: number,
    dataSource: "default" | "withLikes" | null = null
  ): Promise<HttpResult<Horse>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/get`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    baseUrl.searchParams.append('id', String(id));
    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new Horse(), raw.data);
    return raw;
  }
  async like(
        horse: Horse,
    dataSource: "default" | "withLikes" | null = null
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/${this.id}/like`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            horse
      })
    });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    return raw;
  }
  static async list(
    dataSource: "default" | "withLikes" | null = null
  ): Promise<HttpResult<Horse[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/list`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = instantiateObjectArray(raw.data, Horse);
    return raw;
  }
  static async post(
        horse: Horse,
    dataSource: "default" | "withLikes" | null = null
  ): Promise<HttpResult<Horse>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/post`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            horse
      })
    });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new Horse(), raw.data);
    return raw;
  }
}
export class Like {
  id: number;
  horseId1: number;
  horseId2: number;
  horse2: Horse | undefined;

}
