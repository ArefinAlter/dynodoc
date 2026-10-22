import { getConfig } from '$lib/server/config';
import { CANONICAL_ORIGIN, INDEXABLE_PAGES, SITE_IMAGE } from '$lib/seo/site';
import type { LayoutServerLoad } from './$types';

// What the browser may know about this installation: brand, notice, where page metrics go,
// and (hosted only) the search-engine facts the head tags need. Nothing here is secret, and
// nothing reads the request, so the data is fetched once per visit.
export const load: LayoutServerLoad = () => {
  const { site } = getConfig();
  return {
    site,
    seo: site.indexable
      ? {
          origin: CANONICAL_ORIGIN,
          image: SITE_IMAGE,
          paths: INDEXABLE_PAGES.map((page) => page.path)
        }
      : null
  };
};
