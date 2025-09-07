type Ok<T> = { ok: true; data: T };
type Err = { ok: false; status: number; message: string };
type Result<T> = Ok<T> | Err;

export class Person {
  id: number;
  name: string;
  ssn: string | null;

  async speak(
        favorite_number: number
  ): Promise<Result<string>> {
    const url = `http://localhost:5001/api/Person/${this.id}/speak`;

    const res = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            favorite_number
      })
    });

    if (!res.ok) {
      const data = await res.text();
      return {
        ok: false,
        status: res.status,
        message: data
      };
    }

    const data = await res.json();
    return { ok: true, data };
  }
  static async post(
        name: string, 
        ssn: string | null
  ): Promise<Result<string>> {
    const url = `http://localhost:5001/api/Person/post`;

    const res = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            name, 
            ssn
      })
    });

    if (!res.ok) {
      const data = await res.text();
      return {
        ok: false,
        status: res.status,
        message: data
      };
    }

    const data = await res.json();
    return { ok: true, data };
  }
}
