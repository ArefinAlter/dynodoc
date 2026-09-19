<script lang="ts">
  import { ArrowUpRight } from '@lucide/svelte';
</script>

<footer>
  <div class="top">
    <a href="https://dynodoc.online" class="brand"
      ><img src="/brand/mark.svg?v=2" width="32" height="32" alt="" /> dynodoc.</a
    >
    <p>Good research deserves a clear record.</p>
    <a class="source" href="/source">Open source, open possibilities <ArrowUpRight size={15} /></a>
  </div>
  <div class="bottom">
    <span>A shared workspace for thoughtful research.</span>
    <nav aria-label="Footer navigation">
      <a href="https://dynodoc.online/features">Features</a><a
        href="https://dynodoc.online/why-dynodoc">Why Dynodoc</a
      ><a href="https://dynodoc.online/roadmap">Roadmap</a><a href="https://dynodoc.online/help"
        >Help</a
      ><a href="https://dynodoc.online/cookies">Cookies</a><a href="https://dynodoc.online/privacy"
        >Privacy policy</a
      ><a href="https://dynodoc.online/terms">Terms &amp; conditions</a><a
        href="https://github.com/ArefinAlter/dynodoc">GitHub <ArrowUpRight size={12} /></a
      >
    </nav>
  </div>
</footer>

<style>
  footer {
    font-family: var(--font-ui);
    max-width: 1360px;
    padding: 50px 48px 28px;
    margin: auto;
    color: #526275;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 30px;
    padding-bottom: 36px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 24px;
    font-weight: 750;
    letter-spacing: -1px;
    color: #1b3b62;
    text-decoration: none;
  }
  .top p {
    font-size: 14px;
  }
  .source {
    margin-left: auto;
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    text-decoration: none;
    color: #2059a3;
  }
  .bottom {
    display: flex;
    justify-content: space-between;
    gap: 24px;
    border-top: 1px solid #dce0df;
    padding-top: 24px;
    font-size: 13px;
  }
  nav {
    flex-wrap: wrap;
    display: flex;
    gap: 25px;
  }
  nav a {
    display: flex;
    gap: 5px;
    align-items: center;
    color: #526275;
    text-decoration: none;
  }
  nav a:hover,
  .source:hover {
    text-decoration: underline;
    text-underline-offset: 4px;
  }
  @media (max-width: 720px) {
    footer {
      padding: 34px 24px 24px;
    }
    .top {
      flex-wrap: wrap;
      gap: 12px;
      padding-bottom: 24px;
    }
    .top p {
      width: 100%;
      margin: 0;
    }
    .source {
      margin: 12px 0 0;
    }
    .bottom {
      flex-direction: column;
      gap: 16px;
    }
    nav {
      flex-wrap: wrap;
      gap: 20px;
    }
  }
</style>
