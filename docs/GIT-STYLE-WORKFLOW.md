# Working together on one Word file — the Dynodoc workflow

A step-by-step guide for people who have never used Git. Every step is a button in
the Dynodoc workspace. The words in brackets are the Git names, for readers who
know them.

## The idea in one paragraph

There is one shared **team version** of the document. Nobody overwrites it by
accident. Anyone can take a copy, change it on their own computer or in a private
**draft**, and send their changes back as a **change request**. An editor looks at
exactly what changed, decides what to keep where two people changed the same
sentence, and **merges**. Every step is recorded, so earlier text can always be
restored.

| What you do | Dynodoc button | Git name |
| --- | --- | --- |
| Take a copy to edit in Word | File → Download Word (.docx) | clone / checkout |
| Send your edited copy back | File → Upload edited Word file… | push |
| Work privately inside Dynodoc | Personal drafts → Create personal draft | branch |
| Ask the team to review your changes | Send for review | pull request |
| Decide which version of a sentence wins | Review → Keep the team's / Use theirs | resolve conflict |
| Accept the changes into the team version | Merge selected | merge |
| Catch up with newer team changes | Rebase onto latest | rebase |
| Go back to an older version | Version history → Restore as new changes | revert |

## 1. The owner creates or uploads the document

On **Home**, choose **Blank document**, or **Open** to upload an existing Word file.
Share it with the group: **Share** → enter each person's email → **Can edit** (can
merge) or **Can comment & suggest** (can send changes, cannot merge).

## 2. Everyone downloads a copy

**File → Download Word (.docx)**. The file is named like `Group report (v12).docx`.
The `v12` records which version the copy came from, and hidden markers inside the
file remember every paragraph. Rename the file if you like; the markers stay.

## 3. Edit in Word, anywhere

Edit normally in Word — offline is fine. Change sentences, add or delete paragraphs,
fix formatting. Two people can edit their own copies at the same time.

## 4. Upload your edited copy

Open the document in Dynodoc → **File → Upload edited Word file…** → choose your
file. You can also drop it on **Home → Open**: Dynodoc recognises that it belongs to
the existing document and offers **Upload as changes**.

Dynodoc shows:

- **This copy started from** — the version your file came from. Change it if you
  know better (for example, a copy of the original upload from an email).
- how many paragraphs you **edited**, **added**, **removed** or **moved**, with
  removed words in red and new words in green;
- **Name these changes** and **What did you change and why?**

Then choose **Send for review**, or **Open as my personal draft** to keep working
inside Dynodoc first. Nothing in the team version changes yet.

## 5. The editor reviews

The **Change requests** button (next to Personal drafts) shows a number when
requests are waiting. Open a request to see every change:

- **Edited / Added / Removed** changes can be ticked or unticked.
- **Overlaps a team change** appears when the team edited the same paragraph after
  the copy was downloaded. It shows three boxes: the text *when they started*, the
  *team's current version* and *their version*. Pick one.

**Merge N selected** adds the ticked changes to the team version. Unticked changes
stay in the request for later. **Decline** closes the request with an optional
note; the author keeps their draft. **Their file** downloads the uploaded original.

## 6. Private drafts and rebase

**Personal drafts → Create personal draft** gives you a private copy inside
Dynodoc. While you work, the team version may move on; the banner then says *the
team version has N newer changes*. **Rebase onto latest** moves your draft so it
starts from the newest team version and replays your changes on top. If one of your
changes overlaps a newer team edit, choose **Keep mine** or **Use the team's**; you
can also **Drop** changes you no longer want. When ready, **Send for review**.

## 7. Nothing is lost

**Version history** lists named versions; **Restore as new changes** brings old
text back without deleting anything. **Who changed this?** shows who edited a
paragraph and when. Merged changes are recorded as made by the editor who merged
them; the request remembers who wrote them.

## Good to know

- Word files downloaded from Dynodoc match paragraphs exactly. Files from elsewhere
  (or re-saved by tools that drop hidden markers, such as some online editors) are
  matched by wording; a heavily rewritten paragraph may appear as removed + added.
- Page size, margins, headers and footers are not taken from uploaded copies.
- Comments inside Word files are not turned into Dynodoc discussions on upload.
- For group slides, choose **Blank presentation**: each slide and each text box is
  tracked separately, so teammates can suggest changes to different slides at once.

## Try it with test files

1. Create a document with three paragraphs and download it.
2. Make two copies of the file. In copy A change a word in paragraph 2 and add a
   paragraph; in copy B change paragraph 2 differently.
3. Meanwhile edit paragraph 1 in Dynodoc.
4. Upload copy A (merges cleanly), then copy B (shows an overlap on paragraph 2 to
   decide).

The automated version of this exact scenario is `frontend/researcher/e2e/changes.spec.ts`.
