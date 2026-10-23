#!/usr/bin/env python3
"""land.py: lands the current branch on main through the merge queue, and waits for it.

    python3 .github/scripts/land.py "what the branch does" [--no-wait]

Run it in the lane's worktree, on its branch, with everything committed and the local gate
passed. It pushes the branch, opens its pull request (titled "Merge <branch>: <what it does>",
which with " (#N)" is the merge commit's subject) or takes the one already open, and puts it in
the merge queue. The queue runs CI's fast tier on the branch merged onto main and the entries
ahead of it, and merges it with a merge commit, as `git merge --no-ff` would; main moves only to
what it tested. land.py waits until then and exits 0 with the commit main is at, or exits 1 and
says why the pull request left the queue (a check that failed, a conflict with main). GitHub
deletes the branch on origin once it is merged.
"""
import json, subprocess, sys, time

REPO = "indexable-inc/ra-jit"
POLL = 20
GIVE_UP = 3 * 3600


def run(*args, check=True):
    done = subprocess.run(args, capture_output=True, text=True)
    if check and done.returncode:
        sys.exit(f"land.py: {' '.join(args)} failed:\n{done.stderr.strip() or done.stdout.strip()}")
    return done.stdout.strip()


def state(number):
    query = """query($number: Int!) { repository(owner: "indexable-inc", name: "ra-jit") {
      pullRequest(number: $number) { state mergeable isInMergeQueue
        mergeQueueEntry { position state } autoMergeRequest { enabledAt } mergeCommit { oid } } } }"""
    out = run("gh", "api", "graphql", "-f", f"query={query}", "-F", f"number={number}")
    return json.loads(out)["data"]["repository"]["pullRequest"]


def failed_checks(number):
    out = run("gh", "pr", "checks", str(number), "--repo", REPO, "--required", "--json", "name,bucket,link", check=False)
    try:
        return [c for c in json.loads(out or "[]") if c["bucket"] == "fail"]
    except json.JSONDecodeError:
        return []


def queue_runs(number):
    """The merge-queue runs of this pull request that failed, newest first."""
    out = run("gh", "run", "list", "--repo", REPO, "--workflow", "ra-jit.yaml", "--event", "merge_group",
              "--limit", "40", "--json", "headBranch,conclusion,url", check=False)
    runs = json.loads(out or "[]")
    return [r["url"] for r in runs if f"/pr-{number}-" in r["headBranch"] and r["conclusion"] == "failure"]


def main():
    args = [a for a in sys.argv[1:] if a != "--no-wait"]
    if len(args) != 1 or args[0].startswith("-"):
        sys.exit(__doc__)
    what, wait = args[0], "--no-wait" not in sys.argv
    branch = run("git", "rev-parse", "--abbrev-ref", "HEAD")
    if branch in ("HEAD", "main"):
        sys.exit("land.py: check out the lane's branch first; main is moved by the merge queue alone")
    if run("git", "status", "--porcelain", "--untracked-files=no"):
        sys.exit("land.py: commit or drop the changes in the worktree first; the queue lands only what is committed")
    run("git", "fetch", "-q", "origin", "main")
    if not run("git", "rev-list", "origin/main..HEAD"):
        sys.exit(f"land.py: {branch} has nothing that main does not have")

    pushed = subprocess.run(["git", "push", "-q", "-u", "origin", f"HEAD:refs/heads/{branch}"], capture_output=True, text=True)
    if pushed.returncode:
        sys.exit(f"land.py: origin's {branch} has commits this one does not (git pull, or push it yourself):\n{pushed.stderr.strip()}")

    found = json.loads(run("gh", "pr", "list", "--repo", REPO, "--head", branch, "--state", "open", "--json", "number,url"))
    if found:
        number, url = found[0]["number"], found[0]["url"]
    else:
        body = run("git", "log", "--no-merges", "--format=- %s", "origin/main..HEAD")
        url = run("gh", "pr", "create", "--repo", REPO, "--base", "main", "--head", branch,
                  "--title", f"Merge {branch}: {what}", "--body", body)
        number = int(url.rstrip("/").rsplit("/", 1)[1])
    print(f"{url}")
    run("gh", "pr", "merge", str(number), "--repo", REPO, "--merge", "--auto")
    if not wait:
        print("queued; land.py does not wait (--no-wait)")
        return

    said, gone, start = None, 0, time.monotonic()
    while time.monotonic() - start < GIVE_UP:
        pr = state(number)
        if pr["state"] == "MERGED":
            print(f"merged: main is at {pr['mergeCommit']['oid'][:10]} or after it")
            return
        if pr["state"] == "CLOSED":
            sys.exit(f"land.py: {url} was closed")
        if pr["isInMergeQueue"]:
            entry = pr["mergeQueueEntry"] or {}
            now, gone = f"in the merge queue, position {entry.get('position')} ({str(entry.get('state')).lower()})", 0
        elif pr["autoMergeRequest"]:
            if pr["mergeable"] == "CONFLICTING":
                sys.exit(f"land.py: {branch} conflicts with main: merge origin/main into it, then run land.py again")
            failed = failed_checks(number)
            if failed:
                sys.exit("land.py: the pull request's checks failed, so it was not queued:\n"
                         + "\n".join(f"  {c['name']}: {c['link']}" for c in failed))
            now, gone = "waiting for the pull request's checks", 0
        else:
            # Out of the queue and not waiting for it: read twice, as the entry moves between the two.
            gone += 1
            if gone >= 2:
                lines = [f"land.py: {url} left the merge queue without merging."]
                lines += [f"  failed in the queue: {u}" for u in queue_runs(number)[:1]]
                lines += ["  Fix it on the branch, then run land.py again."]
                sys.exit("\n".join(lines))
            now = said
        if now != said:
            print(now, flush=True)
            said = now
        time.sleep(POLL)
    sys.exit(f"land.py: gave up waiting after {GIVE_UP // 3600} hours; {url} may still be queued")


if __name__ == "__main__":
    main()
