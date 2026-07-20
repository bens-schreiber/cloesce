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
            <input id="title" placeholder="title">
            <input id="desc" placeholder="description">
            <button id="create">Create</button>
        </div>
        <h3>Subreddits</h3>
        <div id="subs"></div>
    </div>`),
  );

  app.querySelector<HTMLButtonElement>("#create")!.onclick = async () => {
    if (!need()) return;
    const res = await SubReddit.create(val("title"), val("desc"), auth);
    if (!res.ok) return alert(res.message);
    location.hash = `#/r/${res.data!.id}`;
  };

  // Subreddits are rows in D1, so the directory is just a paged `list` query.
  const subs = app.querySelector("#subs")!;
  const dir = await SubReddit.$list(0, 100);
  const entries = dir.data ?? [];
  if (entries.length === 0) subs.append(el(`<p class="muted">None yet — create one above.</p>`));
  for (const s of entries) {
    subs.append(el(`<div class="card"><a onclick="go('#/r/${s.id}')">r/${s.title}</a></div>`));
  }
}

async function subreddit(id: number) {
  const res = await SubReddit.$get(id);
  if (!res.ok) return app.replaceChildren(el(`<p>Subreddit not found.</p>`));
  const sub = res.data!;

  // One call: the feed hydrates every post out of its own PostDo, each already
  // carrying its metadata and comments.
  const feed = await sub.feed(auth);
  const posts = feed.data ?? [];

  app.replaceChildren(
    el(`<div>
        <h3>r/${sub.title}</h3>
        <p class="muted">${sub.description}</p>
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
    const r = await Post.create(id, val("title"), val("body"), auth);
    if (!r.ok) return alert(r.message);
    subreddit(id);
  };

  const list = app.querySelector("#posts")!;
  for (const p of posts) {
    list.append(
      el(`<div class="card">
            <a onclick="go('#/r/${id}/${p.doId}')"><b>${p.meta.title}</b></a>
            <div class="muted">
                ▲ ${p.meta.upvotes} · u/${p.meta.authorName} · ${p.comments.length} comments
            </div>
        </div>`),
    );
  }
  if (posts.length === 0) list.append(el(`<p class="muted">No posts yet.</p>`));
}

async function postView(subId: number, doId: number) {
  // A Post is addressed by its own globally-unique doId, so reading it does not
  // involve the subreddit at all.
  const res = await Post.$get(doId);
  if (!res.ok) return app.replaceChildren(el(`<p>Post not found.</p>`));
  const post = res.data!;
  const sub = await SubReddit.$get(subId);
  const subTitle = sub.data?.title ?? String(subId);

  app.replaceChildren(
    el(`<div>
        <a onclick="go('#/r/${subId}')">← back to r/${subTitle}</a>
        <div class="card">
            <h3>${post.meta.title}</h3>
            <p>${post.meta.content}</p>
            <div class="muted">
                <span class="vote" id="up">▲</span> ${post.meta.upvotes}
                <span class="vote" id="down">▼</span> · u/${post.meta.authorName}
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
    const r = await post.vote(delta, auth);
    if (!r.ok) return alert(r.message);
    postView(subId, doId);
  };
  app.querySelector<HTMLElement>("#up")!.onclick = () => vote(1);
  app.querySelector<HTMLElement>("#down")!.onclick = () => vote(-1);

  app.querySelector<HTMLButtonElement>("#send")!.onclick = async () => {
    if (!need()) return;
    const r = await Comment.create(doId, val("comment"), auth);
    if (!r.ok) return alert(r.message);
    postView(subId, doId);
  };

  const comments = app.querySelector("#comments")!;
  for (const c of post.comments ?? []) {
    comments.append(
      el(`<div class="card">${c.content}
            <div class="muted">▲ ${c.upvotes} · u/${c.authorName}</div></div>`),
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
      localStorage.setItem("username", (username = res.data!.user.name));
      renderUserBar();
      route();
    };
  }
}

const val = (id: string) => (document.getElementById(id) as HTMLInputElement).value;

function route() {
  const [, kind, subId, postId] = location.hash.split("/"); // "#" , "r", <sub>, <post>
  if (kind === "r" && postId) postView(+subId, +postId);
  else if (kind === "r") subreddit(+subId);
  else home();
}

window.addEventListener("hashchange", route);
renderUserBar();
route();
