<script lang="ts">
  import type { Snippet } from 'svelte';
  import {
    placeholderText,
    resolvedStyle,
    type ShapeFields,
    type ThemeName
  } from '$lib/workspace/presentation';
  let {
    shape,
    theme,
    box,
    prompt = false,
    hideText = false,
    selected = false,
    interactive,
    children
  }: {
    shape: ShapeFields;
    theme: ThemeName;
    box?: Pick<ShapeFields, 'x' | 'y' | 'w' | 'h'>;
    prompt?: boolean;
    hideText?: boolean;
    selected?: boolean;
    /** Editor-only: makes the object a focusable, labelled control. */
    interactive?: {
      id: string;
      label: string;
      onfocus: () => void;
      onpointerdown: (e: PointerEvent) => void;
      ondblclick: () => void;
      onkeydown: (e: KeyboardEvent) => void;
    };
    children?: Snippet;
  } = $props();
  const r = $derived(resolvedStyle(shape, theme));
  const at = $derived(box || shape);
  const lines = $derived(shape.text.split('\n'));
  // Every value is numeric, a validated #rrggbb colour, an enumerated keyword or a theme font.
  const css = $derived(
    [
      `left:${at.x}%`,
      `top:${at.y}%`,
      `width:${at.w}%`,
      `height:${at.h}%`,
      `--pt:${r.size}`,
      `color:${r.color}`,
      r.fill ? `background-color:${r.fill}` : '',
      `text-align:${r.align}`,
      shape.style.bold ? 'font-weight:700' : '',
      shape.style.italic ? 'font-style:italic' : '',
      shape.style.underline ? 'text-decoration:underline' : '',
      `font-family:'${r.font}','Segoe UI',Arial,sans-serif`
    ]
      .filter(Boolean)
      .join(';')
  );
  const classes = $derived(
    [
      'shape',
      shape.kind === 'image' ? 'image' : '',
      r.valign === 'middle' ? 'middle' : '',
      prompt && shape.kind === 'text' && !shape.text ? 'outline' : '',
      selected ? 'selected' : ''
    ]
      .filter(Boolean)
      .join(' ')
  );
</script>

{#snippet body()}
  {#if shape.kind === 'image'}{#if shape.src}<img
        src={shape.src}
        alt={shape.alt || ''}
        draggable="false"
      />{/if}
  {:else if hideText}
    <!-- The in-place editor replaces the rendered text. -->
  {:else if !shape.text && prompt && shape.role}<p class="prompt">
      {placeholderText[shape.role]}
    </p>
  {:else if shape.style.bullets}<ul>
      {#each lines as line, i (i)}<li class:blank={!line.trim()}>
          {#if line}{line}{:else}&#8203;{/if}
        </li>{/each}
    </ul>
  {:else}{#each lines as line, i (i)}<p>
        {#if line}{line}{:else}&#8203;{/if}
      </p>{/each}{/if}
  {@render children?.()}
{/snippet}

{#if interactive}<div
    class={classes}
    style={css}
    role="button"
    tabindex="0"
    aria-label={interactive.label}
    aria-pressed={selected}
    data-shape={interactive.id}
    onfocus={interactive.onfocus}
    onpointerdown={interactive.onpointerdown}
    ondblclick={interactive.ondblclick}
    onkeydown={interactive.onkeydown}
  >
    {@render body()}
  </div>{:else}<div class={classes} style={css}>{@render body()}</div>{/if}

<style>
  .shape {
    position: absolute;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    justify-content: flex-start;
    padding: 0.67cqh 1.33cqh;
    font-size: calc(var(--pt) * 100cqh / 540);
    line-height: 1.18;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .shape.middle {
    justify-content: center;
  }
  .shape.image {
    padding: 0;
  }
  .shape.outline {
    outline: 1px dashed var(--color-subtle);
  }
  .shape.selected {
    /* Drawn on the slide (authored colours), so it keeps one shade in both modes. */
    outline: 1.5px solid #c43e1c;
    outline-offset: 1px;
  }
  .shape:focus-visible {
    outline: 2px solid #185abd;
    outline-offset: 2px;
  }
  p,
  ul {
    margin: 0;
  }
  ul {
    padding-left: 1.1em;
  }
  li.blank {
    list-style: none;
  }
  .prompt {
    color: var(--color-subtle);
  }
  img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: fill;
    user-select: none;
    pointer-events: none;
  }
</style>
