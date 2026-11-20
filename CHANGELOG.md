# Changelog

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
