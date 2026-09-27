<script lang="ts">
  import RibbonButton from './RibbonButton.svelte';
  import {
    defaultSize,
    layouts,
    slideSizes,
    themes,
    type ShapeStyle,
    type Slide,
    type SlideShape,
    type SlideSize,
    type ThemeName
  } from '$lib/workspace/presentation';
  import {
    Plus,
    Copy,
    Trash2,
    Undo2,
    Redo2,
    Bold,
    Italic,
    Underline,
    Baseline,
    PaintBucket,
    AlignLeft,
    AlignCenter,
    AlignRight,
    List,
    BringToFront,
    SendToBack,
    Type,
    ImagePlus,
    Square,
    Play,
    MonitorPlay,
    EyeOff,
    MessageSquarePlus,
    Image,
    Accessibility,
    Palette
  } from '@lucide/svelte';
  let {
    tab = $bindable('Home'),
    editable,
    slide,
    shape,
    theme,
    size,
    canComment,
    canUndo,
    onAction,
    onStyle
  }: {
    tab?: string;
    editable: boolean;
    slide?: Slide;
    shape?: SlideShape;
    theme: ThemeName;
    size: SlideSize;
    canComment: boolean;
    canUndo: boolean;
    onAction: (action: string, value?: string) => void;
    onStyle: (patch: Partial<ShapeStyle>) => void;
  } = $props();
  const tabs = ['Home', 'Insert', 'Design', 'Slide Show', 'Review'];
  let menu = $state(false);
  const textual = $derived(editable && !!shape && shape.kind !== 'image');
  const style = $derived(shape?.style || {});
  function addSlide(layout: string) {
    menu = false;
    onAction('new-slide', layout);
  }
</script>

<nav class="ribbon-tabs" aria-label="Presentation ribbon">
  {#each tabs as name}<button aria-pressed={tab === name} onclick={() => (tab = name)}
      >{name}</button
    >{/each}
</nav>
<div class="ribbon" role="toolbar" aria-label={`${tab} commands`}>
  {#if tab === 'Home' || tab === 'Insert'}
    <div class="group">
      <div class="body">
        <div
          class="menu-anchor"
          role="presentation"
          onfocusout={(e) => {
            if (!e.currentTarget.contains(e.relatedTarget as Node)) menu = false;
          }}
          onkeydown={(e) => {
            if (e.key === 'Escape') menu = false;
          }}
        >
          <button
            class="large"
            aria-haspopup="true"
            aria-expanded={menu}
            disabled={!editable}
            onclick={() => (menu = !menu)}><Plus size={25} strokeWidth={1.5} />New slide ▾</button
          >
          {#if menu}<div class="menu" role="group" aria-label="Choose a layout">
              {#each layouts as layout}<button onclick={() => addSlide(layout.value)}
                  >{layout.label}</button
                >{/each}
            </div>{/if}
        </div>
        {#if tab === 'Home'}<div class="stack">
            <label class="field"
              >Layout<select
                aria-label="Slide layout"
                value={slide?.layout || 'title-content'}
                disabled={!editable || !slide}
                onchange={(e) => onAction('layout', e.currentTarget.value)}
                >{#each layouts as layout}<option value={layout.value}>{layout.label}</option
                  >{/each}</select
              ></label
            >
            <div class="row">
              <RibbonButton
                label="Duplicate slide"
                icon={Copy}
                disabled={!editable || !slide}
                onclick={() => onAction('duplicate-slide')}
              /><RibbonButton
                label="Delete slide"
                icon={Trash2}
                disabled={!editable || !slide}
                onclick={() => onAction('delete-slide')}
              />
            </div>
          </div>{/if}
      </div>
      <span class="label">Slides</span>
    </div>
  {/if}
  {#if tab === 'Home'}
    {#if canUndo}<div class="group">
        <div class="body">
          <div class="stack">
            <RibbonButton
              label="Undo"
              icon={Undo2}
              disabled={!editable}
              onclick={() => onAction('undo')}
            /><RibbonButton
              label="Redo"
              icon={Redo2}
              disabled={!editable}
              onclick={() => onAction('redo')}
            />
          </div>
        </div>
        <span class="label">Undo</span>
      </div>{/if}
    <div class="group">
      <div class="body">
        <div class="stack">
          <div class="row">
            <label class="field"
              >Size<input
                class="size"
                type="number"
                min="6"
                max="200"
                aria-label="Font size in points"
                value={shape ? (style.size ?? defaultSize(shape)) : ''}
                disabled={!textual}
                onchange={(e) => {
                  const value = Number(e.currentTarget.value);
                  if (value >= 6 && value <= 200) onStyle({ size: value });
                }}
              /></label
            >
          </div>
          <div class="row">
            <RibbonButton
              label="Bold"
              icon={Bold}
              active={!!style.bold}
              disabled={!textual}
              onclick={() => onStyle({ bold: !style.bold })}
            /><RibbonButton
              label="Italic"
              icon={Italic}
              active={!!style.italic}
              disabled={!textual}
              onclick={() => onStyle({ italic: !style.italic })}
            /><RibbonButton
              label="Underline"
              icon={Underline}
              active={!!style.underline}
              disabled={!textual}
              onclick={() => onStyle({ underline: !style.underline })}
            /><label class="color" class:disabled={!textual} title="Font color"
              ><Baseline size={16} /><input
                type="color"
                aria-label="Font color"
                value={style.color || '#333333'}
                disabled={!textual}
                onchange={(e) => onStyle({ color: e.currentTarget.value })}
              /></label
            >
          </div>
        </div>
      </div>
      <span class="label">Font</span>
    </div>
    <div class="group">
      <div class="body">
        <div class="stack">
          <div class="row">
            <RibbonButton
              label="Align left"
              icon={AlignLeft}
              active={style.align === 'left'}
              disabled={!textual}
              onclick={() => onStyle({ align: 'left' })}
            /><RibbonButton
              label="Center"
              icon={AlignCenter}
              active={style.align === 'center'}
              disabled={!textual}
              onclick={() => onStyle({ align: 'center' })}
            /><RibbonButton
              label="Align right"
              icon={AlignRight}
              active={style.align === 'right'}
              disabled={!textual}
              onclick={() => onStyle({ align: 'right' })}
            />
          </div>
          <div class="row">
            <RibbonButton
              label="Bullets"
              icon={List}
              active={!!style.bullets}
              disabled={!textual}
              onclick={() => onStyle({ bullets: !style.bullets })}
            />
          </div>
        </div>
      </div>
      <span class="label">Paragraph</span>
    </div>
    <div class="group">
      <div class="body">
        <div class="stack">
          <div class="row">
            <label class="color" class:disabled={!textual} title="Shape fill"
              ><PaintBucket size={16} /><input
                type="color"
                aria-label="Shape fill color"
                value={style.fill || '#ffffff'}
                disabled={!textual}
                onchange={(e) => onStyle({ fill: e.currentTarget.value })}
              /></label
            ><RibbonButton
              label="Bring to front"
              icon={BringToFront}
              disabled={!editable || !shape}
              onclick={() => onAction('front')}
            /><RibbonButton
              label="Send to back"
              icon={SendToBack}
              disabled={!editable || !shape}
              onclick={() => onAction('back')}
            />
          </div>
          <div class="row">
            <button
              class="text"
              disabled={!textual || !style.fill}
              onclick={() => onStyle({ fill: undefined })}>Reset fill</button
            ><RibbonButton
              label="Delete object"
              icon={Trash2}
              disabled={!editable || !shape}
              onclick={() => onAction('delete-shape')}
            />
          </div>
        </div>
      </div>
      <span class="label">Drawing</span>
    </div>
  {:else if tab === 'Insert'}
    <div class="group">
      <div class="body">
        <RibbonButton
          label="Text box"
          icon={Type}
          large
          disabled={!editable || !slide}
          onclick={() => onAction('text-box')}
        /><RibbonButton
          label="Picture"
          icon={ImagePlus}
          large
          disabled={!editable || !slide}
          onclick={() => onAction('image')}
        /><RibbonButton
          label="Rectangle"
          icon={Square}
          large
          disabled={!editable || !slide}
          onclick={() => onAction('rectangle')}
        />
      </div>
      <span class="label">Objects</span>
    </div>
  {:else if tab === 'Design'}
    <div class="group">
      <div class="body themes">
        {#each Object.entries(themes) as [name, t]}<button
            class="theme"
            aria-pressed={theme === name}
            aria-label={`${t.label} theme`}
            title={`${t.label} theme`}
            disabled={!editable}
            style:background-color={t.background}
            style:color={t.title}
            style:font-family={`'${t.font}', 'Segoe UI', Arial, sans-serif`}
            onclick={() => onAction('theme', name)}
            ><span>Aa</span><i style:background-color={t.accent}></i></button
          >{/each}
      </div>
      <span class="label">Themes</span>
    </div>
    <div class="group">
      <div class="body">
        <label class="field"
          >Slide size<select
            aria-label="Slide size"
            value={size}
            disabled={!editable}
            onchange={(e) => onAction('size', e.currentTarget.value)}
            >{#each Object.entries(slideSizes) as [value, s]}<option {value}>{s.label}</option
              >{/each}</select
          ></label
        >
      </div>
      <span class="label">Customize</span>
    </div>
    <div class="group">
      <div class="body">
        <div class="stack">
          <div class="row">
            <label class="color wide" class:disabled={!editable || !slide} title="Background color"
              ><Palette size={16} /><span>Background</span><input
                type="color"
                aria-label="Slide background color"
                value={slide?.background || themes[theme].background}
                disabled={!editable || !slide}
                onchange={(e) => onAction('background', e.currentTarget.value)}
              /></label
            >
          </div>
          <div class="row">
            <button
              class="text"
              disabled={!editable || !slide}
              onclick={() => onAction('background-all')}>Apply to all</button
            ><button
              class="text"
              disabled={!editable || !slide?.background}
              onclick={() => onAction('background-reset')}>Reset</button
            >
          </div>
        </div>
      </div>
      <span class="label">Format background</span>
    </div>
  {:else if tab === 'Slide Show'}
    <div class="group">
      <div class="body">
        <RibbonButton
          label="From beginning"
          icon={Play}
          large
          disabled={!slide}
          onclick={() => onAction('present')}
        /><RibbonButton
          label="From current slide"
          icon={MonitorPlay}
          large
          disabled={!slide}
          onclick={() => onAction('present-current')}
        />
      </div>
      <span class="label">Start slide show</span>
    </div>
    <div class="group">
      <div class="body">
        <RibbonButton
          label={slide?.hidden ? 'Unhide slide' : 'Hide slide'}
          icon={EyeOff}
          large
          active={!!slide?.hidden}
          disabled={!editable || !slide}
          onclick={() => onAction('hide-slide')}
        />
      </div>
      <span class="label">Set up</span>
    </div>
  {:else if tab === 'Review'}
    <div class="group">
      <div class="body">
        <RibbonButton
          label="Comment on slide"
          icon={MessageSquarePlus}
          large
          disabled={!canComment || !slide}
          onclick={() => onAction('comment')}
        />
      </div>
      <span class="label">Comments</span>
    </div>
    <div class="group">
      <div class="body">
        <RibbonButton
          label="Alt text"
          icon={Image}
          large
          disabled={!editable || shape?.kind !== 'image'}
          onclick={() => onAction('alt-text')}
        /><RibbonButton
          label="Check accessibility"
          icon={Accessibility}
          large
          onclick={() => onAction('check')}
        />
      </div>
      <span class="label">Accessibility</span>
    </div>
  {/if}
</div>

<style>
  .ribbon-tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 16px;
    height: 34px;
    background: #f3f3f3;
    flex-shrink: 0;
    overflow-x: auto;
    white-space: nowrap;
  }
  .ribbon-tabs button {
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    padding: 7px 12px;
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
    color: #242424;
    cursor: pointer;
  }
  .ribbon-tabs button[aria-pressed='true'] {
    border-color: #c43e1c;
    font-weight: 600;
  }
  .ribbon {
    display: flex;
    align-items: stretch;
    min-height: 92px;
    flex-shrink: 0;
    margin: 0 7px 5px;
    padding: 5px 2px 3px;
    border: 1px solid #ddd;
    border-radius: 7px;
    background: white;
    overflow-x: auto;
    color: #222;
    font:
      12px 'Segoe UI',
      Arial,
      sans-serif;
  }
  /* Each group is a column: its width is the larger of its commands and its label. */
  .group {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    padding: 2px 10px 1px;
    border-right: 1px solid #d3d3d3;
  }
  .group:last-child {
    border-right: 0;
  }
  .body {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }
  .label {
    margin-top: 3px;
    text-align: center;
    white-space: nowrap;
    font-size: 11px;
    color: #555;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  select,
  input:not([type='color']) {
    font: inherit;
    color: #242424;
    border: 1px solid #c7c7c7;
    background: white;
    border-radius: 2px;
    padding: 3px 4px;
    min-height: 26px;
    max-width: 150px;
  }
  .size {
    width: 50px;
  }
  button.large,
  button.text {
    display: flex;
    align-items: center;
    border: 1px solid transparent;
    background: none;
    border-radius: 3px;
    color: #222;
    font: inherit;
    cursor: pointer;
    white-space: nowrap;
  }
  button.large {
    flex-direction: column;
    gap: 5px;
    min-width: 58px;
    height: 62px;
    padding: 5px 8px;
  }
  button.text {
    padding: 4px 7px;
  }
  button.large:hover,
  button.text:hover,
  button.large[aria-expanded='true'] {
    background: #e7e7e7;
    border-color: #c6c6c6;
  }
  .menu-anchor {
    position: relative;
  }
  .menu {
    position: absolute;
    top: 66px;
    left: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 170px;
    padding: 5px;
    background: white;
    border: 1px solid #d6d6d6;
    border-radius: 6px;
    box-shadow: 0 6px 20px #0002;
  }
  .menu button {
    border: 0;
    background: none;
    text-align: left;
    padding: 8px 10px;
    font: inherit;
    border-radius: 4px;
    cursor: pointer;
  }
  .menu button:hover {
    background: #fbe9e4;
  }
  .themes {
    gap: 6px;
  }
  .theme {
    position: relative;
    width: 58px;
    height: 44px;
    border: 1px solid #c9c9c9;
    border-radius: 3px;
    cursor: pointer;
    font-size: 16px;
    padding: 0;
  }
  .theme i {
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: 5px;
    height: 3px;
  }
  .theme[aria-pressed='true'] {
    outline: 2px solid #c43e1c;
    outline-offset: 1px;
  }
  .color {
    width: 27px;
    height: 27px;
    display: grid;
    place-items: center;
    position: relative;
    border-bottom: 3px solid #c43e1c;
    cursor: pointer;
  }
  .color.wide {
    width: auto;
    display: flex;
    gap: 5px;
    padding: 0 6px;
  }
  .color input {
    position: absolute;
    inset: 0;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }
  .color.disabled {
    opacity: 0.4;
  }
  .color:focus-within,
  button:focus-visible,
  select:focus-visible,
  input:focus-visible {
    outline: 2px solid #c43e1c;
    outline-offset: 1px;
  }
  button:disabled,
  select:disabled,
  input:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
