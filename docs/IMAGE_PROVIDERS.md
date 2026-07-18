# Image provider policy

Online discovery is a first-class feature, but “free images” and “free API use in a wallpaper
application” are different questions. A permissive image license does not override an API's
application restrictions.

Provider terms must be rechecked before implementation and before each public release.

The initial online-provider scope is still images only. Local files may be animated images or
video, but an image provider is not treated as permission to fetch motion content. Any future
online motion catalog needs its own API eligibility, hotlink/download, license, attribution,
storage, and use-reporting review.

## Initial dispositions

| Provider | Disposition | Reason |
| --- | --- | --- |
| Openverse | Candidate default | Searches Creative Commons and public-domain works and supports anonymous API access. License metadata must still be verified and preserved. Prefer `size=large` / dimension filters for multi-monitor spans. |
| Wikimedia Commons | Candidate direct adapter | Strong provenance, structured license metadata, and frequent multi-megapixel originals (museum scans, scientific imagery). Useful when Openverse metadata or originals are incomplete. |
| NASA Image Library | Candidate curated adapter | Excellent extremely large astronomy and Earth imagery via `images-api.nasa.gov`. NASA-authored works are generally public domain in the US, but each record must be checked for third-party material. |
| Smithsonian Open Access | Candidate curated adapter | Millions of CC0 collection images with an official API (`api.data.gov` key). Strong fit for high-resolution stills when media URLs are present. |
| Met Museum Open Access | Candidate curated adapter | Public-domain collection images under CC0 with an official Collection API and high-resolution JPEGs; no API key today. Art-heavy corpus rather than stock photography. |
| Europeana | Candidate aggregator | Rights-labeled European cultural heritage. Filter strictly to public-domain / CC0 / reusable statements; full-resolution files are often hosted by member institutions, so acquisition URLs need per-record validation. |
| Library of Congress | Candidate curated adapter | Large public-domain photographs, maps, and prints with documented APIs. Rights vary by item; prefer clearly marked PD / no-known-restrictions records. |
| Flickr | Deferred | Technically possible when strictly filtering Creative Commons/public-domain results, but requires careful owner-license handling and API compliance. Openverse already indexes suitable Flickr content. |
| Wallhaven | Unknown / deferred | Wallpaper-oriented catalog with an API, but contributor licensing and commercial-redistribution clarity are weak compared with museum/government open-access sources. Do not enable without a dedicated terms review. |
| Unsplash | Disabled pending written approval | Official API guidelines explicitly identify wallpaper applications as replication of the core Unsplash experience. Email `api@unsplash.com` before integrating if seeking an exception. |
| Pexels | Prohibited under published API terms | Official FAQ and API docs disallow wallpaper apps; secondary-feature exception does not fit Easel's primary product purpose. |
| Pixabay | Prohibited under published license | Content License forbids selling or distributing content on a standalone basis, explicitly including wallpaper. |
| Arbitrary websites | Prohibited | No scraping or undocumented API use. |

## Large-image strategy

Stock-photo APIs that once felt wallpaper-friendly (Unsplash historically; Pexels/Pixabay in casual use) now treat dedicated wallpaper clients as out of scope. For multi-monitor physical spans, prioritize sources that:

1. publish an authorized search/download API;
2. attach machine-readable public-domain or reusable license metadata;
3. commonly expose multi-megapixel or print-resolution originals;
4. do not prohibit wallpaper / personal desktop use in API or content terms.

Practical order for adapters after Openverse:

1. **NASA Image Library** — highest density of extremely large modern photographs.
2. **Wikimedia Commons** — breadth plus originals that Openverse may not surface at full resolution.
3. **Smithsonian Open Access** and **Met Museum Open Access** — CC0 museum corpora with official APIs.
4. **Library of Congress** / **Europeana** — additional PD cultural heritage when filters are strict.

Openverse remains the default discovery surface because it already aggregates many CC/PD works (including Flickr and some museum content). Direct adapters earn their place when they add larger originals, cleaner rights metadata, or catalogs Openverse under-represents.

## Openverse requirements

- Query only image media.
- Retain the canonical source URL, creator, provider, license identifier, license URL, and
  attribution text.
- Make license and source filters visible.
- Default to licenses that permit the user's selected usage; do not silently treat all Creative
  Commons variants as interchangeable.
- Warn that Openverse cannot guarantee the accuracy of upstream license metadata and link to the
  original work page.
- Cache results conservatively and honor API rate-limit responses.

Official references:

- https://docs.openverse.org/api/reference/made_with_ov.html
- https://api.openverse.org/v1/
- https://openverse.org/about

## Unsplash decision gate

Unsplash previously appeared in wallpaper clients via older endpoints; current API guidelines
explicitly ban wallpaper applications as replication of the core Unsplash experience. Do not
implement or ship an Unsplash adapter without written authorization for Easel's exact use case.
If authorization is obtained, the adapter must also:

- use returned hotlinked image URLs for display;
- retain the `ixid` parameter when transforming URLs;
- trigger `download_location` when a user sets an image as wallpaper;
- provide photographer and Unsplash attribution with required referral parameters;
- keep access and secret keys confidential, likely through an approved proxy or registration
  flow;
- respect rate limits and any conditions attached to the approval.

Official references:

- https://help.unsplash.com/en/articles/2511245-unsplash-api-guidelines
- https://help.unsplash.com/en/articles/2511257-guideline-replicating-unsplash
- https://unsplash.com/documentation

## Pexels / Pixabay decision gate

Do not implement Pexels or Pixabay adapters while published terms prohibit wallpaper apps
(Pexels) or standalone wallpaper distribution (Pixabay). Written approval would be required to
reopen either disposition, and even then Easel must preserve required attribution and rate-limit
behavior.

Official references:

- https://help.pexels.com/hc/en-us/articles/4405588861721-Can-I-use-the-API-as-a-wallpaper-app
- https://www.pexels.com/api/documentation/
- https://pixabay.com/service/license-summary/
- https://pixabay.com/api/docs/

## Open-access adapter notes

### NASA Image Library

- API root: `https://images-api.nasa.gov`
- Restrict search to `media_type=image`
- Preserve NASA id, title, secondary creator, center, and date
- Reject or quarantine records marked as copyrighted / third-party
- Prefer original or largest listed asset URL from the `/asset/{nasa_id}` manifest

### Wikimedia Commons

- Use the MediaWiki API with a descriptive User-Agent
- Retain license template, artist, credit, and file page URL
- Prefer original file URL over thumbnail derivatives
- Skip files lacking a clear reusable license

### Smithsonian / Met

- Smithsonian: CC0 open-access media via api.data.gov key; only expose records that include a
  media URL
- Met: Collection API Open Access images are CC0; use `primaryImage` / high-res fields and keep
  object page links for provenance

## Normalized asset contract

Every remote asset must include:

```text
provider id and provider asset id
canonical work URL and creator URL
preview and acquisition URLs
native width and height
creator display name
license identifier, URL, and version
attribution text
content-safety classification
provider-specific use-reporting action
metadata retrieval timestamp
media kind (still for the initial provider contract)
```

Unknown license or attribution fields prevent automatic rotation. The user may inspect the
source, but Easel does not infer permission.

## Credentials

Credentials are never stored in profile files. During development they may be supplied by
environment variables; production builds use the operating system's credential store. A future
hosted proxy is a separate product/security decision and is not assumed by this architecture.
