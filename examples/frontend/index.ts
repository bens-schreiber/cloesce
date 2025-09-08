// src/main.ts
import { Person } from "../.generated/client";

let person: Person | null = null;
const out = document.getElementById("out")!;

document.getElementById("create")!.onclick = async () => {
  const res = await Person.post("Alice", "123-45-6789");
  if (res.ok) {
    person = Object.assign(new Person(), res.data);
    out.textContent = `Created ${person.name} (id=${person.id})`;
  } else out.textContent = `Error: ${res.message}`;
};

document.getElementById("speak")!.onclick = async () => {
  if (!person) return;
  const res = await person.speak(42);
  out.textContent = res.ok ? res.data : `Error: ${res.message}`;
};
