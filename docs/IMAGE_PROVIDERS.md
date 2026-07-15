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
| Openverse | Candidate default | Searches Creative Commons and public-domain works and supports anonymous API access. License metadata must still be verified and preserved. |
| Wikimedia Commons | Candidate direct adapter | Strong provenance and structured license metadata; useful when Openverse data is incomplete. |
| NASA Image Library | Candidate curated adapter | Excellent high-resolution material, but each record must be checked because NASA collections can contain third-party material. |
| Flickr | Deferred | Technically possible when strictly filtering Creative Commons/public-domain results, but requires careful owner-license handling and API compliance. Openverse already indexes suitable Flickr content. |
| Unsplash | Disabled pending written approval | Official API guidelines explicitly identify wallpaper applications as replication of the core Unsplash experience. |
| Pexels | Prohibited under published API terms | Official API documentation says wallpaper apps are not supported within API eligibility requirements. |
| Arbitrary websites | Prohibited | No scraping or undocumented API use. |

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

Do not implement or ship an Unsplash adapter without written authorization for Wallspan's exact
use case. If authorization is obtained, the adapter must also:

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
source, but Wallspan does not infer permission.

## Credentials

Credentials are never stored in profile files. During development they may be supplied by
environment variables; production builds use the operating system's credential store. A future
hosted proxy is a separate product/security decision and is not assumed by this architecture.
