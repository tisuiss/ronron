# Changelog

## 0.9.3 — November 21, 2026

- **Pills go back to the right**: once nothing is waiting on you — a WhatsApp message read or replied to (from the island or the phone), a card answered, a turn finished — the pill leaves Ronron's place and goes back to sleep on the right. A "finished" badge no longer holds it on the left.
- **Ronron at rest is Ronron**: in the compact island with no pill at work, the cat on the left is Ronron as he comes in at launch — his own ginger coat and at rest — not the colour of the pill you had open last.
- **Updates in the Ronron window**: Flow → Settings now has the update check too — your version, Check now, Install once a new one is out, and the switch to be told about new versions.
- **The Ronron window's Ronron**: the cat by the name in the sidebar, and the ones walking in the page bands, are now Ronron in his ginger coat with his tail, as at launch, instead of white.

## 0.9.2 — November 21, 2026

- **The compact island at rest**: Ronron alone on the left, your four pills on the right (the main one included). When a pill does something — a session at work or waiting on you, a badge, unread WhatsApp — it leaves the right and takes Ronron's place on the left (at his size when it is the only one).

## 0.9.1 — November 21, 2026

- **More of a session at once**: with a session's pill open, its seven earlier steps show above the two the ticker scrolls.
- **Busy pills on the left**: in the compact island, the pills doing something — a session at work or waiting on you, a badge, unread WhatsApp — sit beside Ronron on the left; the others sleep on the right.

## 0.9.0 — November 20, 2026

- **Ronron for VS Code**: two extensions (`vscode/`, attached to the Windows release) bring the Claude Code sessions of a remote VS Code workspace — Remote Tunnels, Remote-SSH, containers — to the island, through VS Code's own connection. Permissions can be approved from the island. The remote one installs the hooks in the remote's `~/.claude/settings.json` after showing the change, with a dated backup.
- **Chat with a workspace**: the chat's model picker offers **VS Code** and the open remote folders; Claude Code answers there and carries the conversation on in the same session.

## 0.8.2 — November 20, 2026

- **To Do without an account**: not signed in to Microsoft 365, the tasks are kept on this computer (add, tick off, Undo) in a list *On this computer*, which stays beside the account's lists once signed in while it holds tasks.
- **The computer's volume in the Music tab**: a slider and mute for the default output (Windows Core Audio; PipeWire's `wpctl` or PulseAudio's `pactl` on Linux).

## 0.8.1 — November 20, 2026

- **Delete a mail** from the Mail space or the Mail card: it goes to Deleted Items (Microsoft 365 or the classic Outlook), where it can be got back.
- **Calendar week by week**: ‹ and › in the Calendar space, from four weeks back to nine weeks ahead (the calendars are read for that span now), and **Today** to come back. The month view opens on the current month.
- **Ronron's eyes** follow the pointer from left to right only, in the island and on the desktop.
- **Fold when the pointer leaves** (Settings → General, on by default): the open island folds a moment after the pointer leaves it, except while the Ronron window is on screen, a card waits for an answer or something is being typed.

## 0.8.0 — November 20, 2026

- **Several calendars**: the Microsoft 365 account's calendars, and up to eight iCal ones (`webcal://` or `https://`, kept in the credential store), each shown or not and in its own colour (Settings → Integrations → Calendars). Recurring iCal events are expanded in their time zone.
- **Reminders**: per calendar, none or 5, 10, 15, 30 minutes before. The island opens on the event with **Join the meeting** (Teams, Meet, Zoom), **Open** and **OK**.
- **Music tab**: plays, pauses and skips what plays on this computer — Spotify, YouTube Music, Amazon Music — through the system's media controls (Windows) or MPRIS (Linux), with the cover and the progress. No account.
- **Overview in four slots**: with no pill open, the four pills sit side by side, each with its own space; a click opens it as before (its card, the pills in a column).

## 0.7.2 — November 20, 2026

- **Mail space**: the inbox takes the whole width beside the unread count, without the preview tile. A click on a message shows it in the inbox's place, with Reply, Open, Mark as read and Important; **‹ Inbox** (or Esc) goes back.

## 0.7.1 — November 20, 2026

- **A pill open takes the island**: clicking a pill in the overview gives its card the whole width and makes the island as tall as a space; the pills stack in a column on the right, the open one lit. Clicking it again, or the overview's tab, goes back.
- **To Do space**: *Upcoming* and the *Lists* tile are gone. The list shown sits on the right of the title (« Tâches - 4 »): a click picks another list, or Today. ↗ opens To Do.

## 0.7.0 — November 20, 2026

- **A space for each Microsoft 365 tool**: the header's Calendar, Mail and To Do chips become tabs after the overview's. Each opens its own space of three tiles, without pills (the pills stay in the overview):
  - **Calendar**: the date and how long until the next event; this week with each day's events; the next event (or the one clicked) with **Join the meeting** and **Open in Outlook**.
  - **Mail**: the unread count; search, filters and the list; the message clicked, with **Reply** (a draft, nothing is sent), **Open**, **Mark as read** and **Important**.
  - **To Do**: a ring with the tasks ticked off today; today's tasks (or a list's) with Undo and a field to add one; the lists and what is coming.
- The island grows to 300 points for the spaces; Ronron sits at the foot of the left tile.

## 0.6.1 — November 20, 2026

- **Microsoft 365 in the header**: Calendar, Mail and To Do can sit at the top of the island as chips instead of pills (Settings → Integrations → Microsoft 365 → In the header), and take no pill slot. The calendar chip shows the next event (or *Now*), the mail chip the unread count, the To Do chip the tasks left, in red when one is late. A click shows the card.
- The Calendar card says *No events* instead of Cal.com's *No calls scheduled*.

## 0.6.0 — November 20, 2026: Ronron

- **Ronron is born**: a fork of [Coucou](https://github.com/Louis-CFM/coucou) 0.6.1 (Windows and Linux app 0.5.9), renamed Ronron, with its own character, icon and sounds.
- **A cat**: the island's character is now Ronron, a ginger tabby with ears that twitch, a tail that sways, stripes, a pink nose and whiskers. The mini pills are little cats too.
- **« OK Ronron »** wakes the voice assistant (also heard as "ron ron" or "wrong wrong").
- **New sounds**, synthesised by `windows/scripts/gen-sounds.py`: meows, purrs and chimes.
- **App identifier** `ch.cloudguy.ronron`: Ronron keeps its own settings, keys and hooks, and can run next to Coucou.
- **Microsoft 365**: one sign-in in the browser (or with a code) for three pills through Microsoft Graph. **To Do**: what is due first, tick off with Undo, add to your default list. **Calendar** (new): the next events and a month view, with Join for Teams. **Mail**: reads your mailbox through Graph instead of the classic Outlook, on Windows and Linux; never sends (plan mails go to Drafts without Outlook).
- Only the Windows and Linux app remain: the Mac, iPhone and Android apps, the website and the relay were removed.

Coucou's history up to the fork is in [its changelog](https://github.com/Louis-CFM/coucou/blob/main/CHANGELOG.md).
