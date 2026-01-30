# GitHub Webhook Setup Guide

GitHub integration lets Archon respond to issue comments, PR comments, and @mentions via webhooks.

## 1. Set Up a Public URL (ngrok)

GitHub webhooks need to reach your local server. Check if ngrok is installed:

```bash
which ngrok
```

**If not installed**, use **AskUserQuestion**:

```
Header: "Install ngrok"
Question: "ngrok is not installed. Want me to install it via Homebrew?"
Options:
  1. "Yes, install it" (Recommended) — runs `brew install ngrok`
  2. "I'll install it myself" — user handles it, wait for confirmation
```

If yes, run:
```bash
brew install ngrok
```

**If ngrok is not authenticated**, check and guide:
```bash
ngrok config check 2>&1
```

If it needs auth:
1. Tell the user: "Sign up at https://ngrok.com (free tier works), then copy your auth token from the dashboard."
2. Use **AskUserQuestion** to ask for the token, then run:
```bash
ngrok config add-authtoken <token>
```

## 2. Start ngrok

Tell the user to run this in a **separate terminal** (ngrok must stay running):

```
Run this in another terminal:  ngrok http 3090
```

Then use **AskUserQuestion**:
```
Header: "ngrok URL"
Question: "Paste the ngrok HTTPS URL from the other terminal (e.g., https://abc123.ngrok-free.app):"
Options:
  1. "I'll paste it" — user provides the URL
```

Store the URL as `<ngrok-url>`.

## 3. Generate a Webhook Secret

```bash
openssl rand -hex 32
```

Store this as `<webhook-secret>`.

## 4. Generate a GitHub Token

1. Tell the user to go to [github.com/settings/tokens](https://github.com/settings/tokens)
2. Generate a **fine-grained token** with:
   - Repository access for the target repo
   - Permissions: Issues (read/write), Pull Requests (read/write), Contents (read)

Use **AskUserQuestion** to collect the token when ready.

## 5. Add to `.env` (in the archon repo root)

Write these values to `.env`:

```env
WEBHOOK_SECRET=<webhook-secret>
GITHUB_TOKEN=<token from step 4>
GH_TOKEN=<same token>
GITHUB_ALLOWED_USERS=<user's GitHub username>
GITHUB_STREAMING_MODE=batch
```

## 6. Configure the Repository Webhook

Tell the user to go to their **target repo** on GitHub > **Settings** > **Webhooks** > **Add webhook** and configure:

- **Payload URL**: `<ngrok-url>/webhooks/github`
- **Content type**: `application/json`
- **Secret**: `<webhook-secret>` (the value from step 3)
- Select events: **Issue comments** + **Pull request review comments** (or "Send me everything")
- Click **Add webhook**

Use **AskUserQuestion** to confirm when done.

## 7. Verify the Webhook

Start the server and test the webhook endpoint:

```bash
cd <archon-repo> && bun run dev &
sleep 3
curl -s http://localhost:3090/health
```

If health check returns `{"status":"ok"}`, also verify the ngrok tunnel is forwarding:

```bash
curl -s <ngrok-url>/health
```

Both should return `{"status":"ok"}`. If the ngrok check fails, make sure the ngrok terminal is still running.

Stop the background server when done verifying:
```bash
kill %1 2>/dev/null
```

## Notes

- **Free tier URLs change on restart** — you'll need to update the webhook URL in GitHub each time you restart ngrok.
- **Persistent URLs**: Use a paid ngrok plan, Cloudflare Tunnel, or cloud deployment (see `docs/cloud-deployment.md`).
- Both the **server** (`bun run dev`) and **ngrok** must be running for GitHub webhooks to work.
