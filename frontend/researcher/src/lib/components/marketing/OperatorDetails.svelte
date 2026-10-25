<script lang="ts">
  import type { Snippet } from 'svelte';
  import { EMAIL_HREF, OPERATOR } from '$lib/site/operator';
  // The operator block shared by the legal pages. It renders inside `.prose`, which styles
  // `.contact-details`. `children` adds rows (for example governing law) to the same list.
  let { emailLabel = 'Email', children }: { emailLabel?: string; children?: Snippet } = $props();
</script>

<dl class="contact-details">
  <dt>Legal operator</dt>
  <dd>{OPERATOR.legalName}</dd>
  <dt>Registered address</dt>
  <dd>{OPERATOR.address}</dd>
  <dt>Company registration</dt>
  <dd>{OPERATOR.companyRegistration}</dd>
  <dt>Trade licence</dt>
  <dd>{OPERATOR.tradeLicence}</dd>
  <dt>{emailLabel}</dt>
  <dd><a href={EMAIL_HREF}>{OPERATOR.email}</a></dd>
  <dt>Phone</dt>
  <dd><a href={OPERATOR.phoneHref}>{OPERATOR.phone}</a></dd>
  {@render children?.()}
</dl>

<style>
  /* Registration numbers and addresses must wrap inside the box on a phone. */
  dd {
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
