import { Comment, Post, SubReddit, User } from "@cloesce/client.js";

// --- session --------------------------------------------------------------
let token = localStorage.getItem("token");
let username = localStorage.getItem("username");

// Attach the session token to authed requests (client takes a fetch as last arg).
const auth: typeof fetch = (input, init = {}) => {
  const headers = new Headers(init.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  return fetch(input, { ...init, headers });
};

// --- tiny DOM + routing helpers -----------------------------------------------
const app = document.getElementById("app")!;
const userBar = document.getElementById("user")!;

const el = (html: string): HTMLElement => {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild as HTMLElement;
};

(window as any).go = (hash: string) => (location.hash = hash);
const need = (): boolean => {
  if (token) return true;
  alert("Log in first.");
  return false;
};

// --- views --------------------------------------------------------------------
async function home() {
  app.replaceChildren(
    el(`<div>
        <div class="card">
            <h3>Create a subreddit</h3>
            <input id="name" placeholder="name">
            <input id="desc" placeholder="description">
            <button id="create">Create</button>
        </div>
        <h3>Subreddits</h3>
        <div id="subs"></div>
    </div>`),
  );

  app.querySelector<HTMLButtonElement>("#create")!.onclick = async () => {
    if (!need()) return;
    const res = await SubReddit.create({ name: val("name"), description: val("desc") }, auth);
    if (!res.ok) return alert(res.message);
    location.hash = `#/r/${res.data!.subId}`;
  };

  // The global subreddit directory (a Worker KV index written on create).
  const subs = app.querySelector("#subs")!;
  const dir = await SubReddit.list();
  const entries = dir.data?.results ?? [];
  if (entries.length === 0) subs.append(el(`<p class="muted">None yet — create one above.</p>`));
  for (const s of entries) {
    subs.append(el(`<div class="card"><a onclick="go('#/r/${s.subId}')">r/${s.name}</a></div>`));
  }
}

async function subreddit(id: string) {
  const sub = await SubReddit.$get(id);
  const meta = sub.data?.metadata.value ?? { name: id, description: "" };
  const posts = await Post.$list(id, 0, 100);

  app.replaceChildren(
    el(`<div>
        <h3>r/${meta.name}</h3>
        <p class="muted">${meta.description}</p>
        <div class="card">
            <input id="title" placeholder="post title">
            <textarea id="body" placeholder="text"></textarea>
            <button id="post">Post</button>
        </div>
        <div id="posts"></div>
    </div>`),
  );

  app.querySelector<HTMLButtonElement>("#post")!.onclick = async () => {
    if (!need()) return;
    const res = await Post.create(id, val("title"), val("body"), auth);
    if (!res.ok) return alert(res.message);
    subreddit(id);
  };

  const list = app.querySelector("#posts")!;
  for (const p of posts.data ?? []) {
    list.append(
      el(`<div class="card">
            <a onclick="go('#/r/${id}/${p.id}')"><b>${p.title}</b></a>
            <div class="muted">▲ ${p.upvotes} · u/${p.author}</div>
        </div>`),
    );
  }
  if ((posts.data ?? []).length === 0) list.append(el(`<p class="muted">No posts yet.</p>`));
}

async function postView(subId: string, postId: number) {
  const res = await Post.$get(subId, postId);
  if (!res.ok) return app.replaceChildren(el(`<p>Post not found.</p>`));
  const post = res.data!;
  const sub = await SubReddit.$get(subId);
  const subName = sub.data?.metadata.value?.name ?? subId;

  app.replaceChildren(
    el(`<div>
        <a onclick="go('#/r/${subId}')">← back to r/${subName}</a>
        <div class="card">
            <h3>${post.title}</h3>
            <p>${post.content}</p>
            <div class="muted">
                <span class="vote" id="up">▲</span> ${post.upvotes}
                <span class="vote" id="down">▼</span> · u/${post.author}
            </div>
        </div>
        <div class="card">
            <input id="comment" placeholder="add a comment">
            <button id="send">Comment</button>
        </div>
        <h4>Comments</h4>
        <div id="comments"></div>
    </div>`),
  );

  const vote = async (delta: number) => {
    if (!need()) return;
    const r = await post.vote(subId, delta, auth);
    if (!r.ok) return alert(r.message);
    postView(subId, postId);
  };
  app.querySelector<HTMLElement>("#up")!.onclick = () => vote(1);
  app.querySelector<HTMLElement>("#down")!.onclick = () => vote(-1);

  app.querySelector<HTMLButtonElement>("#send")!.onclick = async () => {
    if (!need()) return;
    const r = await Comment.create(subId, postId, val("comment"), auth);
    if (!r.ok) return alert(r.message);
    postView(subId, postId);
  };

  const comments = app.querySelector("#comments")!;
  for (const c of post.comments ?? []) {
    comments.append(
      el(`<div class="card">${c.content}
            <div class="muted">▲ ${c.upvotes} · u/${c.author}</div></div>`),
    );
  }
}

// --- login + router -----------------------------------------------------------
function renderUserBar() {
  if (username) {
    userBar.replaceChildren(el(`<span>u/${username} · <a id="logout">log out</a></span>`));
    userBar.querySelector("#logout")!.addEventListener("click", () => {
      localStorage.clear();
      token = username = null;
      renderUserBar();
      route();
    });
  } else {
    userBar.replaceChildren(
      el(`<span><input id="u" placeholder="username" style="width:auto;display:inline">
            <button id="login">Log in</button></span>`),
    );
    userBar.querySelector<HTMLButtonElement>("#login")!.onclick = async () => {
      const res = await User.login(val("u"));
      if (!res.ok) return alert(res.message);
      localStorage.setItem("token", (token = res.data!.token));
      localStorage.setItem("username", (username = res.data!.user.username));
      renderUserBar();
    };
  }
}

const val = (id: string) => (document.getElementById(id) as HTMLInputElement).value;

function route() {
  const [, kind, subId, postId] = location.hash.split("/"); // "#" , "r", <sub>, <post>
  if (kind === "r" && postId) postView(subId, +postId);
  else if (kind === "r") subreddit(subId);
  else home();
}

window.addEventListener("hashchange", route);
renderUserBar();
route();
