// Static, first-party metadata only: no visitor or document values enter this HTML.
export const structuredDataScript =
  '<script type="application/ld+json">' +
  JSON.stringify({
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'WebSite',
        '@id': 'https://dynodoc.online/#website',
        name: 'Dynodoc',
        url: 'https://dynodoc.online/'
      },
      {
        '@type': 'SoftwareApplication',
        name: 'Dynodoc',
        url: 'https://dynodoc.online/',
        applicationCategory: 'ProductivityApplication',
        operatingSystem: 'Web browser',
        description:
          'A shared workspace for research documents, spreadsheets and questionnaires with block-based version history.',
        image: 'https://dynodoc.online/brand/social.png'
      }
    ]
  }) +
  '</script>';
