import type { RequestHandler } from './$types';
import { CANONICAL_ORIGIN, DISALLOWED_PATHS, isIndexableHost, PUBLIC_SEO_FILES } from '$lib/seo';

// Crawlers that retrieve pages to cite them in generated answers, and those
// that collect training data. Both are allowed: being found and quoted
// accurately is the point of the llms.txt files below.
const AI_AGENTS = [
  'OAI-SearchBot',
  'ChatGPT-User',
  'GPTBot',
  'ClaudeBot',
  'Claude-SearchBot',
  'Claude-User',
  'anthropic-ai',
  'PerplexityBot',
  'Perplexity-User',
  'Google-Extended',
  'Applebot-Extended',
  'CCBot',
  'cohere-ai',
  'Meta-ExternalAgent',
  'Bytespider',
  'Amazonbot',
  'DuckAssistBot',
  'MistralAI-User',
  'YouBot'
];

const CRAWL_DELAY_AGENTS = ['Bytespider', 'Amazonbot'];

function allowGroup(agent: string): string {
  const delay = CRAWL_DELAY_AGENTS.includes(agent) ? 'Crawl-delay: 10\n' : '';
  return `User-agent: ${agent}\nAllow: /\n${DISALLOWED_PATHS.map((p) => `Disallow: ${p}`).join('\n')}\n${delay}`;
}

const PUBLIC_ROBOTS = [
  `# ${CANONICAL_ORIGIN}`,
  '# Private research content lives on app.dynodoc.online and admin.dynodoc.online,',
  '# which serve a fully disallowed robots.txt of their own.',
  '',
  'User-agent: *',
  'Allow: /',
  ...DISALLOWED_PATHS.map((path) => `Disallow: ${path}`),
  '',
  '# Answer engines: allowed to read and cite these pages.',
  '',
  ...AI_AGENTS.map(allowGroup),
  '# Machine-readable summaries for answer engines.',
  ...PUBLIC_SEO_FILES.filter((file) => file.startsWith('/llms')).map(
    (file) => `# ${CANONICAL_ORIGIN}${file}`
  ),
  '',
  `Sitemap: ${CANONICAL_ORIGIN}/sitemap.xml`,
  ''
].join('\n');

// Every other host (app., admin., preview deployments) is private, and so is every
// self-hosted installation.
const PRIVATE_ROBOTS = 'User-agent: *\nDisallow: /\n';

export const GET: RequestHandler = ({ url, locals }) =>
  new Response(
    locals?.indexable !== false && isIndexableHost(url.hostname) ? PUBLIC_ROBOTS : PRIVATE_ROBOTS,
    {
      headers: {
        'content-type': 'text/plain; charset=utf-8',
        'cache-control': 'public, max-age=3600'
      }
    }
  );
