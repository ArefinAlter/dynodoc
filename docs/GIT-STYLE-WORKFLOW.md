# Working together on one file — the Dynodoc workflow

## Projects front page (10 October)

Sign in to open **Projects**. Your profile contains a name and optional short bio,
visible only to people sharing a Project with you. Open **Edit profile** to change
them. Profile pages do not show your email address.

Create a Project and use **Add a file** for each document, workbook or presentation.
Test uploads must be no larger than **3 MB (3,000,000 bytes)**. Each row opens a
read-only file preview; **Pull requests**, **History** and **Sync & connections**
manage the team's changes. **Web editor** opens the existing editor separately.
The global **Editors** link also opens files outside Projects.

The file's branch selector starts on **main**, its shared version. **New file draft**
creates private work; use **Compare with main** and **Open pull request** when ready.
Drafts currently belong to a single file. Project-wide branches and atomic multi-file
commits are still planned. **History → Recover an earlier revision** provides native
downloads and selected restore/revert/cherry-pick proposals, subject to permissions.
Native downloads preserve the supported content; retain originals for unsupported
formatting and embedded objects. The source/deployment status is in
[PROJECT-STATUS.md](PROJECT-STATUS.md).

## Projects and external editors (development increment, 6 October)

Dynodoc's priority is collecting and reviewing changes from your existing editors.
Word/Docs, Excel/Sheets and PowerPoint/Slides development connectors share the
workflow; the web editors remain available. Native host acceptance and marketplace
publication are separate checks. See [current release status](SPREADSHEET-DATE-SYNC-RELEASE.md).

On a matching installation, open **Projects**, create a
project and add/move your files. **Files** collects them, **Pull requests** lists
submitted edits across those files, **Your drafts** shows only your unsubmitted
work, and **Settings** manages people/default review rules. Files inherit the
project's rules unless their People panel shows an override. Watch requests adds
in-app notices when someone submits work; invitation email remains optional.

**Sync & connections** lets you download the team checkpoint, upload a portable
change bundle, create an editor-specific seven-day connection key and revoke it.
The development panes/sidebars can opt into capture of existing paragraph text,
spreadsheet cells, date-only values/formulas or slide shape text, retain observations privately,
push them for review and preview team fields before applying them in the editor. Use the ordinary file
push workflow for formatting, inserted/deleted paragraphs and other unsupported
structures. [Installation and real-host acceptance](../extensions/README.md).

Date-only cells keep the same calendar day in Excel and Google Sheets. Invalid
dates, hidden times and unsupported date formats stop sync before writing. Date
format changes and timestamps use file push. Existing Sheets sidebar installations
need the new `Date.gs` file; Excel needs API 1.13. Preview and apply team changes
explicitly, just as for other supported cells.

Capturing a snapshot does not prove who typed its text or record every keystroke.
An upload receipt identifies the authenticated uploader; merging identifies the
responsible merger. These are distinct from the client-reported local observations.

A step-by-step guide for people who have never used Git. Every step is a button in
the Dynodoc workspace. The words in brackets are the Git names, for readers who
know them. It covers Word documents, Excel workbooks and PowerPoint decks.

## The idea in one paragraph

There is one shared **team version** of each file. Nobody overwrites it by accident.
Anyone with access can take a copy, change it on their own computer or in a private
**draft**, and send their changes back as a **change request**. Reviewers approve
or ask for changes; an editor or the owner looks at exactly what changed, decides
what to keep where two people changed the same sentence, and **merges**. Every step
is recorded, so retained earlier text can be recovered. If someone uploads a copy of
your file — even renamed — you are told.

| What you do | Dynodoc button | Git name |
| --- | --- | --- |
| Take a copy to edit in Word, Excel or PowerPoint | File → Download | clone / checkout |
| Send your edited copy back | File → Upload edited … file | push |
| Work privately inside Dynodoc | Personal drafts → Create personal draft | branch |
| Ask the team to review your changes | Send for review | pull request |
| Say whether a request is ready | Approve / Request changes | review |
| Decide which version of a sentence wins | Keep the team's / Use theirs | resolve conflict |
| Accept the changes into the team version | Merge selected | merge |
| Catch up with newer team changes | Rebase onto latest | rebase |
| Recover earlier content | Version history → Recover earlier content | restore selected content |
| Protect the team version and require approvals | Share → Review rules | branch protection |

## Recover something removed earlier

Open **Version history**, enter the retained revision number and choose **Preview
recovery**. Activity also offers **Recover before this edit**. The revision can be
older than the visible activity list and does not need a named version.

Choose the blocks to bring back, then **Create recovery draft**. Nothing changes
for the team yet. Inspect the draft, choose **Send for review**, and use the normal
approval and merge process. Unselected blocks keep their current content. Changes
made after the draft was created are compared during merge; overlaps need review.
Recovering a parent or child may require selecting the related blocks together.

**Download checkpoint JSON** saves the selected historical state and chain anchor.
It is data for inspection/integration, not a native Office file. Viewers may inspect
and download; contributors may propose recovery; current owner/project rules decide
who approves and merges. Previously erased history is unavailable. This feature
restores whole block states; replaying or reverting an individual commit is future work.

## Who can do what

Open a file and choose **Share**. The panel shows the **owner**, everyone with
access and how they got it, and what *you* can do.

| Role | Can do |
| --- | --- |
| **Viewer** | Read and download. |
| **Contributor** | Everything a viewer can, plus comment, suggest, work in personal drafts and send changes (including edited files) for review. |
| **Reviewer** | Everything a contributor can, plus **Approve** or **Request changes** on other people's requests. |
| **Editor** | Edit the team version directly, and merge or decline change requests — within the review rules. |
| **Manager** | Given through a workspace (organization, team or folder). Shares the file, sets review rules, can override them. |
| **Owner** | One person — whoever created or uploaded the file, until they transfer it. Everything a manager can do, and is told when someone uploads a copy. |

The owner and managers add people with **Add people** (email + role), change a
person's role from the list, and move ownership with **Transfer ownership** (the
new owner must already have access; the previous owner stays an editor).
Adding someone also emails them an invitation (untick **Send an email invitation
with a link** to skip it; add an optional message). The email names who shared
what, the role, a link to the file and how to sign in with that address; replies
go to the person who shared. The message under the form says whether it was sent.
The same person is emailed at most once every 10 minutes for the same file, and one
person can send at most 30 invitations an hour. Changing a role in the list does not
send an email. Sharing a workspace (Home → Options → Access & folders) works the same
way.
Workspace roles (Home → Options → Access & folders) offer the same ladder:
Owner, Manager, Editor, Reviewer, Contributor, Viewer.

### Review rules

Under **Share → Review rules** the owner or a manager chooses:

- **Protect the team version** — only the owner and managers change it directly.
  Editors see *“The team version is protected”* and a **Start a draft** button;
  their work goes through change requests like everyone else's.
- **Approvals needed before merging** — none, or 1–5. Approvals from reviewers,
  editors, managers or the owner count; nobody can approve their own request.
  If a reviewer asks for changes, the request waits until they approve.
  An approval stops counting when the author changes the request afterwards.
- **Who merges** — editors, managers and the owner, or only the owner and managers.

The owner and managers can still merge a request without the required approvals
(the button then says **Merge without approvals**); the audit log records that.

## Notifications

The **bell** at the top of Home and of every file shows a red number for unread
notices. You are told when:

- someone uploads a copy of a file you own (see below);
- someone sends changes for review in a file you own, edit or review;
- someone approves, asks for changes, comments on, merges or declines your request;
- someone gives you access, or makes you the owner.

Click a notice to open the right file, request or list. **Mark all read** clears
the number. Notices appear within a few seconds. Only invitations are also sent by
email; every other notice is in the bell only.

## 1. The owner creates or uploads the file

On **Home**, choose **Blank document** / **workbook** / **presentation**, or **Upload a
file** (the first tile, the **Upload** button in the left bar, or drop a file anywhere
on Home) to bring in an existing Word, Excel or PowerPoint file. Share it with the group from
**Share** and pick each person's role.

## 2. Everyone downloads a copy

**File → Download Word (.docx)**, **Download Excel (.xlsx)** or **Download PowerPoint
(.pptx)**. The file is named like `Group report (v12).docx`. The `v12` records which
version the copy came from, and hidden information inside the file remembers it
too. Rename the file if you like.

## 3. Edit on your computer

Edit normally in Word, Excel or PowerPoint — offline is fine. Two people can edit
their own copies at the same time.

## 4. Upload your edited copy

Open the file in Dynodoc → **File → Upload edited Word / Excel / PowerPoint file…**
→ choose your file. You can also drop it on **Home** or use **Home → Upload**: Dynodoc compares its
*content* with every file you can open, so it recognises your copy even if you
renamed it, and offers **Upload as changes** next to the matching file.

Dynodoc shows:

- **This copy started from** — the version your file came from. Change it if you
  know better (for example, a copy of the original upload from an email).
- how many paragraphs, rows or slides you **edited**, **added**, **removed** or
  **moved**, with removed words in red and new words in green;
- a warning if the file shares very little with this document — it may be the wrong
  file, or a different file with the same name (see *Same name, different content*);
- **Name these changes** and **What did you change and why?**

Then choose **Send for review**, or **Open as my personal draft** to keep working
inside Dynodoc first. Nothing in the team version changes yet.

## 5. Reviewers and editors review

The **Change requests** button shows a number when requests are waiting. Open one:

- **Edited / Added / Removed** changes can be ticked or unticked.
- **Overlaps a team change** appears when the team edited the same paragraph after
  the copy was downloaded. It shows three boxes: the text *when they started*, the
  *team's current version* and *their version*. Pick one.
- **Comment** beside any change leaves a note on that change; the **Reviews** section
  shows every approval, request for changes and comment, and how many approvals the
  rules still need.
- Reviewers use **Approve** or **Request changes** (with a note).

**Merge N selected** adds the ticked changes to the team version. Unticked changes
stay in the request for later. **Decline** closes the request with an optional note;
the author keeps their draft. **Their file** downloads the uploaded original. The
author is notified either way.

## 6. Same name, different content

If someone uploads a *different* file under the same name — for example their own
`report.docx` that has nothing to do with yours — Dynodoc does not quietly treat it
as an edit:

- On **Home → Upload** they see *“A document named … already exists, but its content
  is different.”*
- If they still upload it as changes, the request is marked **This file shares
  little with this document**, and every existing paragraph (row, text box) it would
  replace or remove shows as **Replaces a team paragraph**. Nothing merges until you
  choose **Keep the team's** or **Use theirs** for each one, or use **Keep all team
  paragraphs** / **Use all of theirs**. New paragraphs can be ticked as usual.
- If the uploader already works with you, the owner also gets a *same name,
  different content* notice.

## 7. When someone uploads a copy of your file

Every uploaded file is compared, by content, with every file in Dynodoc — even if it
was renamed, re-saved or partly edited, and even if the person who uploaded it
cannot open your original. If it is a copy of a file you own, you get a notice like
*“Tariq uploaded ‘report - Tariq.docx’, which is an edited copy of ‘Group report’
(89% match)”*. You then choose:

- **Invite them to send changes** — they get Contributor access and a notice with
  **Send my copy as changes**, which opens your file with their copy ready to upload
  as a change request;
- **Known, it's fine** (in **Share → Copies and similar files**) or **Dismiss**.

What you learn is who uploaded which file name and how similar it is — never its
content. The uploader sees a note about this before every upload. Copies made with
**File → Make a copy** by people who already had access are listed but not
announced. **Share → Copies and similar files** lists every copy of your file.

## 8. Private drafts and rebase

**Personal drafts → Create personal draft** gives you a private copy inside
Dynodoc. While you work, the team version may move on; the banner then says *the
team version has N newer changes*. **Rebase onto latest** moves your draft so it
starts from the newest team version and replays your changes on top. If one of your
changes overlaps a newer team edit, choose **Keep mine** or **Use the team's**; you
can also **Drop** changes you no longer want. When ready, **Send for review**.

## 9. Nothing is lost

**Version history** lists named versions; **Restore as new changes** brings old
text back without deleting anything. **Who changed this?** shows who edited a
paragraph and when. Merged changes are recorded as made by the person who merged
them; the request remembers who wrote them.

## Day and night mode

The sun/moon button at the top of every page switches between **day**, **night** and
**match my computer**. The choice follows you across the website, the workspace
and the admin site. In night mode, pages and spreadsheet cells go dark; text you
coloured black shows light, and highlighted text stays readable. Slides keep their
own design, as in PowerPoint. To check how a document will print, use **View →
White pages**. Printing and downloaded files always use normal colours.

## For administrators: permanently deleting files

On the admin site, **People, content & access** opens on **Documents**. Use **Show →
In Trash** to find binned files. Either:

- press **Delete** on one row — the dialog shows exactly what will be removed; a
  file that is not in Trash is moved there and deleted in one step; or
- tick several rows and press **Delete permanently…**, then type `DELETE 3
  DOCUMENTS` (with your number) to confirm.

Each deleted file gets its own deletion receipt (**Browse → Deletion receipts**).
Backups keep data until they expire.

## Good to know

- Word files downloaded from Dynodoc match paragraphs exactly. Files from elsewhere,
  and all Excel and PowerPoint files, are matched by content; a heavily rewritten
  paragraph, row or slide may appear as removed + added.
- Page size, margins, headers and footers are not taken from uploaded copies.
- Comments inside Word files are not turned into Dynodoc discussions on upload.
- For group slides, each slide and each text box is tracked separately, so teammates
  can suggest changes to different slides at once.

## Try it: a test plan for 2–3 people

Use separate browsers (for example Chrome, a Chrome incognito window and Edge) so
each person has their own sign-in.

1. **Renamed copy reaches the owner.** Olivia creates *Group report* with five
   paragraphs and downloads it. Tariq (no access) renames his copy `report -
   Tariq.docx`, edits a sentence and uploads it on **Home → Upload** as a new
   document. Within a few seconds Olivia's bell shows the notice; she presses
   **Invite them to send changes**. Tariq's bell offers **Send my copy as changes**;
   he sends it for review; Olivia merges.
2. **Same name, different content.** Tariq uploads an unrelated file named `Group
   report.docx`. Home warns him; if he sends it anyway, Olivia's review shows
   **Replaces a team paragraph** for every existing paragraph and needs a choice.
3. **Roles and rules.** Olivia gives Eli **Editor**, Rina **Reviewer** and Tariq
   **Contributor**, then turns on **Protect the team version** and **1 approval**.
   Eli sees *The team version is protected* and works in a draft; Tariq sends a
   request; Rina approves; Eli merges. Try merging before Rina approves: only
   Olivia can, with **Merge without approvals**.
4. **Overlaps.** Two copies change the same paragraph differently while Olivia edits
   another: one request merges cleanly; the other shows an overlap to decide.
5. **Excel and PowerPoint.** Download a workbook, insert a row and change a cell in
   Excel, then **Upload edited Excel file…**: one row added, one edited, nothing
   else. Do the same with a deck (edit a title, reorder slides).
6. **Night mode.** Switch with the sun/moon button on Home, in a document, a
   spreadsheet, a deck, the admin site and the public website.
7. **Administrator deletion.** Delete one active file and a batch from Trash.

Automated versions: `frontend/researcher/e2e/copies.spec.ts` (tests 1–3 and the
protected draft), `e2e/changes.spec.ts` (test 4), `structured-push.test.ts`
(test 5 on real Excel/PowerPoint round trips) and
`crates/engine-api/tests/copies_test.rs` (detection, roles, approvals, unrelated
pushes and deletion against a real database).


## Revert, cherry-pick and download earlier content

Open **Version history**. Choose **Revert changes in a range** to undo the net
content changes in that range, or **Cherry-pick changes in a range** to apply them
again. The start revision is excluded and the end revision is included. Preview,
select the blocks you need, and choose between current and historical values for
any overlaps. Create a private recovery draft, inspect it and send it for review.
Merging adds new history; it does not delete the intervening work. These commands
currently use retained revision ranges, not portable commit IDs or other branches.

**Download native file** reconstructs the requested revision as DOCX, XLSX or PPTX,
with its document/revision origin. It exports Dynodoc's supported content; keep the
original file for unsupported embedded parts and exact formatting. You can also
download the checkpoint JSON. Current file permissions apply to both downloads.

Install editor integrations from **Sync & connections ? Install editor add-ins**
or `/connectors`. Use the pane's **Sign in to Dynodoc** link, approve a specific file
in the browser, return to finish and enable capture. Packages are development/test
installations until native acceptance and marketplace publication are complete.
