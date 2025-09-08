import { Person } from "../.generated/client";

let person: Person | null = null;
const out = document.getElementById("out")!;
const speakButton = document.getElementById("speak")!;

document.getElementById("personForm")!.onsubmit = async (e) => {
  e.preventDefault();

  const name = (document.getElementById("name") as HTMLInputElement).value;
  const ssn = (document.getElementById("ssn") as HTMLInputElement).value;

  const res = await Person.post(name, ssn);
  if (res.ok) {
    person = Object.assign(new Person(), res.data);
    out.textContent = `Created ${person.name} (id=${person.id})`;
    speakButton.removeAttribute("disabled");
  } else {
    out.textContent = `Error: ${res.message}`;
  }
};

speakButton.onclick = async () => {
  if (!person) return;
  const res = await person.speak(42);
  out.textContent = res.ok ? res.data : `Error: ${res.message}`;
};
