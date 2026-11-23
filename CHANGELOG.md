# Changelog

## 0.13.0 — November 23, 2026

- **Aircall**: a new service pill (Settings → Accounts & services → Integrations → Aircall, with the company's API ID and token). Its card shows today's calls — in, out, missed — and the last three; click it for the last 20 with who took each. **Call back** opens the number in your computer's phone app (the Aircall app when it handles phone links), **To Do** turns a missed call into a « Call back » task, and a new missed call or voicemail lights the pill. « OK Ronron, who called me? » reads them. Read-only, every 30 s while the pill is on.
- **The Music tab's volume is back for more players**: an app whose name is several words of its id (the YouTube Music desktop app) is found again, and so is a player on an output that is not the default one (a headset). When none is found, ronron.log says which apps had a sound stream.

## 0.12.3 — November 23, 2026

- **« OK Ronron » with Gemini answers**: Gemini refused the whole request because one of Ronron's tools (your services' data) takes no parameters; such a tool is now sent without a schema. Thinking models (Gemini 2.5 and later) also get room to think and still answer.
- **The pill says why** when Ronron can't answer — the provider's own message (a refused key, a model not found, a quota) instead of always "check your key".

## 0.12.2 — November 23, 2026

- **« OK Ronron » with another model than Claude**: Voice → *Ronron answers with* picks Claude (your key), Gemini, OpenAI, OpenRouter, or a model on your computer — Ollama, LM Studio, any OpenAI-compatible server — with the key and the model already set for it in AI agents → Chat. Gemini with a Google AI Studio key or Ollama costs nothing. Pills, music, mail, reminders and your services' data work with all of them; the web search with Claude and Gemini (Google Search, same key). The voice page says what is missing (a key, a model) instead of always asking for a Claude key.
- **A volume for each kind of sound**: Ronron → Appearance & sounds keeps the general volume and adds three sliders — Notifications (an agent finishes, asks or fails, a message, a reminder, an alarm), Clicks and interface, and Ronron's own sounds. Letting go of one plays a sound at its new level; at 0 % that kind is silent.
- The messages that still sent you to "Notch → Chat" now say AI agents → Chat.

## 0.12.1 — November 22, 2026

- **A clean uninstall when you ask for it**: on Windows, ticking *Delete the application data* in the uninstaller now also clears every key Ronron stored in the Credential Manager (Microsoft, Google, API keys, WhatsApp…) and removes `%APPDATA%\Ronron` and `%LOCALAPPDATA%\Ronron` (settings, reminders, local tasks, voice models). Unticked — and always on an update — everything stays for a reinstall.

## 0.12.0 — November 22, 2026

- **Reminders**: a new tab in the island's header (Settings → Accounts & services → Integrations → Reminders) for reminders kept on this computer, no account needed. Write what to remember, pick in 10 min, in 1 h, this evening, tomorrow morning or a date and time; when its time comes the island opens on it with **In 10 min**, **In 1 h** and **Done**. One that came due while the computer slept rings as soon as Ronron is back.
- **« OK Ronron, remind me to… »** adds a reminder by voice, and lists or removes them.
- **Microsoft 365 in one click**: the sign-in no longer asks for a domain; by default it goes through Microsoft's « Microsoft Graph Command Line Tools » app, for any account. **Use my own app registration…** takes your own Entra app (and its tenant) instead.

## 0.11.2 — November 21, 2026

- **To Do's ring counts the tasks you see**: with nothing due, Today shows every open task — its ring now counts them too (ticked off today, out of that plus the open ones), and the picker's *Today* number matches the list. It used to read 2/2 with two tasks still waiting.
- **Claude Code in a remote VS Code workspace reaches Ronron again**: with an older VS Code window still running on the same folder, the remote hook could pick that window, whose tunnel to your computer was gone, and the events went nowhere. It now picks the newest window on the folder (Ronron for VS Code remote 0.9.1).

## 0.11.1 — November 21, 2026

- **The closed island you picked, at once**: once the pointer leaves the compact island — after a fold, a hover or the launch — what Settings → Behavior → *Closed island shows* asks for comes back after a second, instead of the compact island staying for a minute. A request or a message that brings the island back still keeps it out for a minute.
- **No empty band in the compact island**: without the clock it is 200 px wide instead of 288 — Ronron, the plan and the pills, nothing in between. With the clock on, it keeps its room in the middle.

## 0.11.0 — November 21, 2026

- **Settings, arranged by what you set**: the Ronron window's sidebar now reads Ronron (appearance & sounds), Island (behavior, active pills, shortcuts), AI agents (agents, chat), Accounts & services (accounts, integrations, Android phone), Voice (dictation, Notetaker, statistics, words, styles, OK Ronron, dictation settings), Mail and System (startup, language, updates, weekly recap). "Notch" and "Flow" are gone from the PC's names, the General page is split by subject, and Updates lives in one place.
- **Accounts**: Microsoft 365, Google and the calendars have a page of their own, apart from the integrations.
- **Ronron on every tab**: the open island shows Ronron in his ginger coat and outfit whatever the pill — the pills' colours stay on their own little cats.

## 0.10.0 — November 21, 2026

- **Google accounts**: Settings → Integrations → **Google** signs in with your own Google Cloud project and feeds the Mail, Calendar and To Do tabs from **Gmail**, **Google Calendar** and **Google Tasks** — beside Microsoft 365 or instead of it. Signed in to both, the inboxes are merged (each message's actions go back to its own account), and the calendars and task lists sit side by side.
- **Gmail in Ronron's words**: Important is the star, Junk is Spam, a rule's folder is a label, Delete goes to the Trash; replies and the chat's mails are saved in Gmail's Drafts, never sent by Ronron.

## 0.9.4 — November 21, 2026

- **The Music tab's volume is the player's own**: the slider and mute now set only the app playing (Spotify, the browser with YouTube Music, Amazon Music…) — its stream in the Windows mixer, or in PulseAudio / PipeWire on Linux — and no longer the computer's volume.
- **The chat is as tall as To Do**: it opens at the size of the To Do space and the other header spaces, instead of growing with the conversation.

## 0.9.3 — November 21, 2026

- **Pills go back to the right**: once nothing is waiting on you — a WhatsApp message read or replied to (from the island or the phone), a card answered, a turn finished — the pill leaves Ronron's place and goes back to sleep on the right. A "finished" badge no longer holds it on the left.
- **Ronron at rest is Ronron**: in the compact island with no pill at work, the cat on the left is Ronron as he comes in at launch — his own ginger coat and at rest — not the colour of the pill you had open last.
- **A closed island stays closed**: only a request (a permission, a question) or a WhatsApp message brings it back. A session at work or finishing, a deploy, music starting no longer do — the island keeps what Settings → Closed island shows, and the pill keeps its badge.
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
