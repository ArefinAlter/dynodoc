<script lang="ts">
  import EditorShell from '$lib/components/editor/EditorShell.svelte';
  import { sampleDocument } from '$lib/mock/editor';

  let { params } = $props();
  const document = $derived({ ...sampleDocument, id: params.documentId });
</script>

<EditorShell {document} />
