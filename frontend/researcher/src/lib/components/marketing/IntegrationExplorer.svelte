<script lang="ts">
  import { onMount } from 'svelte';
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  import {
    ArrowRight,
    ArrowUpRight,
    Download,
    FileSpreadsheet,
    Database,
    Check,
    Plug
  } from '@lucide/svelte';
  const integrations = [
    {
      name: 'ODK Central',
      logo: 'odk.svg',
      url: 'https://getodk.org',
      type: 'Direct import',
      headline: 'Bring fieldwork into the conversation.',
      description:
        'Browse projects and forms on your ODK Central server. Import an available XLSForm or a snapshot of submissions into a workspace your team can review.',
      value:
        'Keep a reviewable copy of the instrument or response data beside the decisions that shape your study.',
      format: 'XLSForm · submission snapshots',
      note: 'Uses your server URL and a token for this request. Imports are snapshots; changes are not written back to ODK.'
    },
    {
      name: 'KoboToolbox',
      logo: 'kobo.png',
      url: 'https://www.kobotoolbox.org',
      type: 'Direct import',
      headline: 'Give your Kobo project a shared working space.',
      description:
        'Connect to your KoboToolbox server, choose a form and bring in its XLSForm or response snapshot for review, editing and analysis preparation.',
      value:
        'Move from collection to team review while retaining the imported source and its origin.',
      format: 'XLSForm · response snapshots',
      note: 'Uses your server URL and a token for this request. Repeat imports create new snapshots, without automatic synchronization.'
    },
    {
      name: 'REDCap',
      logo: 'redcap.svg',
      url: 'https://projectredcap.org',
      type: 'File import',
      headline: 'Review your data dictionary as a team.',
      description:
        'Upload a REDCap data dictionary CSV to work with question text, field names, choices and survey metadata in Dynodoc.',
      value: 'Make instrument revisions easier to discuss before you update the study in REDCap.',
      format: 'Data dictionary CSV',
      note: 'File compatibility only. Dynodoc does not connect to the REDCap API or execute REDCap branching logic.'
    },
    {
      name: 'SurveyCTO',
      logo: 'surveycto.webp',
      url: 'https://www.surveycto.com',
      type: 'File import',
      headline: 'Give the next field round a clearer starting point.',
      description:
        'Bring in a SurveyCTO-compatible XLSForm workbook to review its survey structure, question wording and choices together.',
      value:
        'Keep the team’s proposed wording changes in context before returning to your collection workflow.',
      format: 'XLSForm workbook',
      note: 'File compatibility only. Advanced calculations and constraints need validation in your collection platform before deployment.'
    },
    {
      name: 'SurveyMonkey',
      logo: 'surveymonkey.svg',
      url: 'https://www.surveymonkey.com',
      type: 'File import',
      headline: 'Make exported responses easier to work through.',
      description:
        'Upload a CSV or XLSX export to a spreadsheet workspace. Organize columns, use formulas and review changes with the rest of your team.',
      value: 'Keep data preparation decisions visible as responses move toward analysis.',
      format: 'CSV · XLSX',
      note: 'Tabular response imports only. No direct SurveyMonkey API connection or recreation of hosted survey behavior.'
    },
    {
      name: 'CSPro',
      logo: '',
      url: 'https://www.census.gov/data/software/cspro.html',
      type: 'File import',
      headline: 'Keep the next stage of data preparation connected.',
      description:
        'Export tabular data from your CSPro workflow as CSV or XLSX, then upload it to a Dynodoc spreadsheet for collaborative review.',
      value: 'Give exported field data a working history your team can return to.',
      format: 'CSV · XLSX',
      note: 'Tabular files only. Native CSPro applications, dictionaries and logic are not imported.'
    }
  ];
  let selected = $state(0);
  const current = $derived(integrations[selected]);
</script>

<div class="integrations">
  <div class="logos" aria-label="Explore research integrations">
    {#each integrations as item, i}<button
        disabled={!ready}
        aria-pressed={selected === i}
        aria-label={`Explore ${item.name} integration`}
        onclick={() => (selected = i)}
        ><div
          class="logo"
          class:redcap={item.name === 'REDCap'}
          class:odk={item.name === 'ODK Central'}
        >
          {#if item.logo}<img
              src={`/brands/${item.logo}`}
              alt=""
              loading="lazy"
              width="120"
              height="40"
            />{:else}<Database size={26} /><b>CSPro</b>{/if}
        </div>
        <span>{item.name}</span><small
          >{item.type === 'Direct import' ? 'Connect & import' : 'Upload a file'}</small
        ></button
      >{/each}
  </div>
  <div class="integration-detail" aria-live="polite">
    <div class="detail-copy">
      <span class="connection-label"
        >{#if current.type === 'Direct import'}<Plug size={13} />{:else}<FileSpreadsheet
            size={13}
          />{/if}{current.type}</span
      >
      <h3>{current.headline}</h3>
      <p>{current.description}</p>
      <p class="value"><Check size={16} />{current.value}</p>
      <a href={current.url}>About {current.name} <ArrowUpRight size={14} /></a>
    </div>
    <div class="import-diagram">
      <div class="diagram-label">FROM FIELDWORK TO SHARED UNDERSTANDING</div>
      <div class="source-node">
        <Download size={21} /><span><b>{current.name}</b><small>{current.format}</small></span>
      </div>
      <div class="connector" aria-hidden="true"><span></span><ArrowRight size={18} /></div>
      <div class="destination-node">
        <FileSpreadsheet size={22} /><span
          ><b>Your Dynodoc workspace</b><small>Review · edit · compare · share</small></span
        >
      </div>
      <p>{current.note}</p>
    </div>
  </div>
  <p class="trademark-note">
    Compatibility, without changing where you collect data. Product names and logos belong to their
    respective owners; no affiliation or endorsement is implied.
  </p>
</div>

<style>
  .logos {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    border-top: 1px solid #dddcd6;
    border-bottom: 1px solid #dddcd6;
  }
  .logos button {
    border: 0;
    border-bottom: 3px solid transparent;
    background: transparent;
    padding: 24px 12px 20px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    color: #314b63;
    min-width: 0;
  }
  .logos button:hover {
    background: #f2f5f5;
  }
  .logos button[aria-pressed='true'] {
    background: #ecf3fc;
    border-bottom-color: #2864b7;
  }
  .logo {
    height: 44px;
    width: 125px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    margin-bottom: 14px;
    max-width: 100%;
    color: #2e5680;
  }
  .logo img {
    width: 110px;
    height: 40px;
    object-fit: contain;
    max-width: 100%;
  }
  .logo.odk img {
    height: 45px;
    width: 85px;
  }
  .logo.redcap {
    background: transparent;
    border-radius: 3px;
    padding: 4px 8px;
  }
  .logo.redcap img {
    height: 34px;
  }
  .logo b {
    font-size: 20px;
    letter-spacing: -0.5px;
    font-weight: 700;
  }
  .logos button > span {
    font-size: 12px;
    font-weight: 600;
  }
  .logos small {
    font-size: 10px;
    color: #6e7d88;
    margin-top: 5px;
  }
  .integration-detail {
    display: grid;
    grid-template-columns: 1.15fr 1fr;
    gap: 65px;
    padding: 46px 35px 35px;
    background: #fffefa;
    border: 1px solid #e2e1db;
    border-top: 0;
    border-radius: 0 0 6px 6px;
    min-height: 385px;
  }
  .connection-label {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    color: #2862a7;
  }
  .detail-copy h3 {
    font-size: 29px;
    font-weight: 550;
    letter-spacing: -1px;
    line-height: 1.2;
    color: #263f58;
    max-width: 390px;
    margin: 19px 0;
  }
  .detail-copy p {
    font-size: 12px;
    line-height: 1.9;
    color: #617284;
  }
  .detail-copy .value {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    color: #375a7f;
    font-size: 11px;
    margin: 20px 0;
  }
  .value :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
  }
  .detail-copy a {
    font-size: 11px;
    display: inline-flex;
    gap: 6px;
    align-items: center;
    color: #245ba4;
    text-decoration: none;
  }
  .detail-copy a:hover {
    text-decoration: underline;
  }
  .import-diagram {
    align-self: center;
    background: #f2f6fb;
    border: 1px solid #dbe5f0;
    border-radius: 6px;
    padding: 25px 28px;
  }
  .diagram-label {
    font-size: 8px;
    letter-spacing: 1px;
    color: #698097;
    margin-bottom: 25px;
  }
  .source-node,
  .destination-node {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
    background: white;
    border: 1px solid #d3e0f0;
    border-radius: 5px;
    color: #2961a3;
  }
  .source-node b,
  .destination-node b {
    display: block;
    font-size: 12px;
    font-weight: 550;
  }
  .source-node small,
  .destination-node small {
    display: block;
    font-size: 9px;
    color: #6f859a;
    margin-top: 3px;
  }
  .connector {
    height: 37px;
    position: relative;
    display: flex;
    align-items: center;
    margin-left: 30px;
    color: #91b3dd;
  }
  .connector > span {
    display: block;
    width: 1px;
    height: 37px;
    border-left: 1px dashed #8aafd9;
  }
  .connector > :global(svg) {
    position: absolute;
    transform: rotate(90deg);
    left: -8px;
    background: #f2f6fb;
  }
  .destination-node {
    background: #e8f1ff;
    border-color: #c3d7f4;
  }
  .import-diagram > p {
    font-size: 10px;
    line-height: 1.8;
    color: #647c93;
    margin: 22px 0 0;
  }
  .trademark-note {
    font-size: 10px;
    color: #727d86;
    line-height: 1.8;
    margin-top: 18px;
    max-width: 800px;
  }
  @media (prefers-reduced-motion: no-preference) {
    .logos button {
      transition: background 0.2s;
    }
    .connector > :global(svg) {
      animation: import 1.5s ease-in-out 3;
    }
    @keyframes import {
      0%,
      100% {
        translate: 0 -4px;
        opacity: 0.5;
      }
      50% {
        translate: 0 5px;
        opacity: 1;
      }
    }
  }
  @media (max-width: 800px) {
    .logos {
      grid-template-columns: repeat(3, 1fr);
    }
    .logos button {
      padding: 20px 8px 16px;
    }
    .integration-detail {
      gap: 30px;
      padding: 32px 25px;
    }
    .detail-copy h3 {
      font-size: 25px;
    }
    .import-diagram {
      padding: 20px;
    }
    .diagram-label {
      line-height: 1.8;
    }
  }
  @media (max-width: 560px) {
    .logo {
      width: 95px;
      height: 35px;
      margin-bottom: 10px;
    }
    .logo img {
      width: 90px;
      height: 30px;
    }
    .logo.odk img {
      height: 36px;
    }
    .logo.redcap img {
      height: 26px;
    }
    .logos button > span {
      font-size: 10px;
    }
    .logos small {
      font-size: 8px;
    }
    .logos button {
      padding: 18px 6px 14px;
    }
    .logo b {
      font-size: 17px;
    }
    .integration-detail {
      grid-template-columns: 1fr;
      padding: 25px 20px;
      gap: 23px;
    }
    .detail-copy h3 {
      font-size: 26px;
      max-width: 300px;
    }
    .detail-copy p {
      font-size: 12px;
    }
    .import-diagram {
      padding: 22px;
    }
    .trademark-note {
      font-size: 9px;
    }
  }
</style>
