import { Horse, Like } from "../../.generated/client";

// App State
let horses: Horse[] = [];
let currentIndex = 0;
let currentUserHorse: Horse | null = null;

// DOM Elements
const browseView = getElement("browse-view");
const addView = getElement("add-view");
const listView = getElement("list-view");
const horseCard = getElement("horse-card");
const horsesList = getElement("horses-list");
const messages = getElement("messages");
const messageText = getElement("message-text");
const currentUserIdInput = getElement<HTMLInputElement>("current-user-id");

// Helper to get elements with type safety
function getElement<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Element with id "${id}" not found`);
  return element as T;
}

// Navigation
getElement("btn-browse").addEventListener("click", () => {
  showView("browse");
  loadHorsesForBrowsing();
});

getElement("btn-add").addEventListener("click", () => showView("add"));

getElement("btn-list").addEventListener("click", () => {
  showView("list");
  loadAllHorses();
});

// Browse Actions
getElement("btn-pass").addEventListener("click", nextHorse);
getElement("btn-like").addEventListener("click", likeCurrentHorse);

// Add Horse Form
getElement<HTMLFormElement>("add-horse-form").addEventListener(
  "submit",
  async (e) => {
    e.preventDefault();
    if (e.currentTarget instanceof HTMLFormElement) {
      await addNewHorse(e.currentTarget);
    }
  }
);

// Matches
getElement("btn-matches").addEventListener("click", async () => {
  if (!currentUserHorse) {
    showMessage("Set your Current User ID first!", true);
    return;
  }

  const matches = await currentUserHorse.matches();
  if (matches.ok) {
    showMatches(matches.data!);
  } else {
    showMessage(`Error loading matches: ${matches.message}`, true);
  }
});

// Functions
function showView(view: "browse" | "add" | "list"): void {
  browseView.style.display = "none";
  addView.style.display = "none";
  listView.style.display = "none";
  hideMessage();

  const views = { browse: browseView, add: addView, list: listView };
  views[view].style.display = "block";
}

function showMessage(text: string, isError = false): void {
  messageText.textContent = `${isError ? "❌" : "✅"} ${text}`;
  messages.style.display = "block";
  setTimeout(hideMessage, 3000);
}

function hideMessage(): void {
  messages.style.display = "none";
}

async function loadHorsesForBrowsing(): Promise<void> {
  try {
    const currentUserId = parseInt(currentUserIdInput.value);

    const userResult = await Horse.get(currentUserId, "default");
    if (userResult.ok && userResult.data) {
      currentUserHorse = userResult.data;
    }

    const result = await Horse.list("default");
    if (result.ok && result.data) {
      horses = result.data.filter((h) => h.id !== currentUserId);
      currentIndex = 0;
      displayCurrentHorse();
    } else {
      horseCard.innerHTML = "<p><strong>Error loading horses</strong></p>";
    }
  } catch (error) {
    horseCard.innerHTML = `<p><strong>Error: ${
      (error as Error).message
    }</strong></p>`;

    throw error;
  }
}

function displayCurrentHorse(): void {
  if (horses.length === 0) {
    horseCard.innerHTML =
      "<p><strong>No horses available!</strong></p><p>Add some horses to get started.</p>";
    return;
  }

  if (currentIndex >= horses.length) {
    horseCard.innerHTML =
      "<p><strong>No more horses!</strong></p><p>You've seen them all. Refresh to start over.</p>";
    return;
  }

  const horse = horses[currentIndex];
  const likesInfo =
    horse.likes?.length > 0
      ? `<p><em>Likes: ${horse.likes
          .map((l) => l.horse2?.name ?? "Unknown")
          .join(", ")}</em></p>`
      : "";

  horseCard.innerHTML = `
    <h3>${horse.name}</h3>
    <dl>
      <dt><strong>ID:</strong></dt>
      <dd>${horse.id}</dd>
      <dt><strong>Bio:</strong></dt>
      <dd>${horse.bio ?? "<em>No bio provided</em>"}</dd>
    </dl>
    ${likesInfo}
    <p><small>Horse ${currentIndex + 1} of ${horses.length}</small></p>
  `;
}

function nextHorse(): void {
  currentIndex++;
  displayCurrentHorse();
}

async function likeCurrentHorse(): Promise<void> {
  if (currentIndex >= horses.length || !currentUserHorse) return;

  const targetHorse = horses[currentIndex];

  try {
    const result = await currentUserHorse.like(targetHorse);
    if (result.ok) {
      showMessage(`You liked ${targetHorse.name}! 💕`);
      nextHorse();
    } else {
      showMessage("Failed to like horse", true);
    }
  } catch (error) {
    showMessage(`Error: ${(error as Error).message}`, true);
  }
}

async function addNewHorse(form: HTMLFormElement): Promise<void> {
  const formData = new FormData(form);
  const horse = {
    name: formData.get("name") as string,
    bio: (formData.get("bio") as string) || null,
  } as Horse;

  try {
    const result = await Horse.save(horse);
    if (result.ok) {
      showMessage(`${horse.name} added successfully!`);
      form.reset();
    } else {
      showMessage("Failed to add horse", true);
    }
  } catch (error) {
    showMessage(`Error: ${(error as Error).message}`, true);
  }
}

async function loadAllHorses(): Promise<void> {
  try {
    const result = await Horse.list("default");
    if (result.ok && result.data) {
      displayAllHorses(result.data);
    } else {
      horsesList.innerHTML = "<p><strong>Error loading horses</strong></p>";
    }
  } catch (error) {
    horsesList.innerHTML = `<p><strong>Error: ${
      (error as Error).message
    }</strong></p>`;
  }
}

function displayAllHorses(horseData: Horse[]): void {
  if (horseData.length === 0) {
    horsesList.innerHTML = "<p><em>No horses yet. Add some!</em></p>";
    return;
  }

  horsesList.innerHTML = horseData
    .map((horse) => {
      const likesInfo =
        horse.likes?.length > 0
          ? `<li><em>Likes: ${horse.likes
              .map((l) => l.horse2?.name ?? "Unknown")
              .join(", ")}</em></li>`
          : "";

      return `
        <article>
          <h3>${horse.name}</h3>
          <ul>
            <li><strong>ID:</strong> ${horse.id}</li>
            <li><strong>Bio:</strong> ${horse.bio ?? "<em>No bio</em>"}</li>
            ${likesInfo}
          </ul>
          <hr>
        </article>
      `;
    })
    .join("");
}

function showMatches(matches: Horse[]): void {
  if (matches.length === 0) {
    horseCard.innerHTML = "<p><strong>No matches yet!</strong></p>";
    return;
  }

  horseCard.innerHTML = `
    <h3>💖 Your Matches</h3>
    <ul>
      ${matches
        .map(
          (horse) =>
            `<li><strong>${horse.name}</strong> (ID: ${horse.id})<br><em>${
              horse.bio ?? "No bio"
            }</em></li>`
        )
        .join("")}
    </ul>
  `;
}

// Initialize
showView("browse");
loadHorsesForBrowsing();
