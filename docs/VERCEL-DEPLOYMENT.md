# Deploy the homepage to Vercel

The same SvelteKit app can build for a VPS or Vercel. The ten-person workspace runs on the VPS so its document event streams and file converter have persistent services. Vercel serves the public homepage and links into the workspace.

If using Vercel's Git integration, first commit and push the current implementation
to the branch you will import. Vercel cannot deploy uncommitted local changes from
your GitHub repository. The VPS deployment does not push code to GitHub.

1. Import this repository in your Vercel account. Select **SvelteKit** and set the **Root Directory** to `frontend/researcher`.
2. Use Node.js **24.x**, install command `pnpm install --frozen-lockfile`, and build command `pnpm build`. Keep the framework's default output directory. The Svelte configuration selects the Vercel adapter when Vercel sets `VERCEL=1`.
3. Add this environment variable for production and preview builds:

   ```dotenv
   PUBLIC_WORKSPACE_URL=https://dynodoc.103.191.50.32.sslip.io
   ```

4. Deploy. The default `*.vercel.app` address is enough. Check the navigation, four-step interactive explanation, mobile layout and reduced-motion preference. **Open workspace** and **Sign in** should lead to the HTTPS VPS workspace.
5. Configure Google and Resend on the VPS using [the sign-in guide](GOOGLE-AND-EMAIL-SETUP.md). Do not copy the database, signing or email secrets to the homepage project.

Vercel deployment is intentionally left to the owner, as requested. A Vercel homepage deployment alone does not deploy the PostgreSQL database, Rust API or Word converter.

## Isolated VPS operation

```sh
cd /opt/dynodoc-demo
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml ps
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml logs --tail 100 web api
./infra/demo/backup.sh
```

The project's containers, network and data volume use the `dynodoc-demo` prefix. Only the web service publishes a host port, `127.0.0.1:3088`. Nginx handles the dedicated HTTPS hostname. The database, API and conversion service are not publicly exposed.

Backups are PostgreSQL custom archives containing documents, events, versions, identities and original uploaded files. The backup script validates the archive and keeps 14 days. Copy backups to another host or object storage before relying on this VPS for irreplaceable research data; off-site backup is not configured by this repository.

Restore into a new database for an exercise, never over the live database:

```sh
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml exec -T postgres createdb -U dynodoc dynodoc_restore_check
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml exec -T postgres pg_restore -U dynodoc -d dynodoc_restore_check < backups/YOUR_BACKUP.dump
```

Check table counts and chain verification before switching a deployment to any restored database. Keep `infra/demo/.env` separate from backups and protect it as a secret. Do not run `docker compose down -v` on the live deployment.

## Updating the isolated deployment

From the working repository, create a source package:

```sh
python3 infra/demo/package-source.py /tmp/dynodoc-source.tar.gz
```

The script packages tracked and new non-ignored source files, excludes private
`.env` files, dependencies and build output, and includes a SHA-256 manifest.
Review it before transfer. Copy the archive to the VPS, extract it into
`/opt/dynodoc-demo`, and retain a copy at `/opt/dynodoc-demo/source.tar.gz`.
The web service mounts that archive read-only for the homepage's source download.
Keep it in sync with the deployed build. Extraction preserves the separately
configured `infra/demo/.env` because the archive excludes it.

```sh
cd /opt/dynodoc-demo
sha256sum -c SOURCE_MANIFEST.sha256
./infra/demo/backup.sh
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml build
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml up -d
```

After an update, verify `/healthz`, anonymous document rejection, sign-in and a
synthetic document edit. Check the existing VPS applications independently. Never
use an unscoped stop/reset command on this shared host.
