# Mediacenter — corrected desktop design brief

I would like your help refining the visual design of a personal Linux desktop media-center application called **Mediacenter**. This brief was corrected on 5 November 2026 after the first proposals misunderstood the primary display and controls.

**This request is for appearance and visual concepts only. Do not build the application or write an implementation plan. Refine the selected Colour Wall direction in light and dark themes before application development begins.**

## What I am trying to create

Mediacenter is installed on my Linux computer and its control interface stays on the computer. I use it as the central place to manage my media. When I connect an HDMI cable, the television is an optional playback display for video and audio. Do not design Mediacenter as a television application or assume its controls move onto the television.

The main reason for this project is watching my streaming services through the computer's VPN connection. As an additional use, I want access to my own videos, photos, and music stored on the computer or on an external hard drive or flash drive connected through USB-A or USB-C. Mediacenter should be a visually polished, practical desktop control center for those activities.

I have NordVPN installed on the computer. One reason for using the computer is that I cannot install a VPN on my LG television or configure one on my router. Streaming played on the computer would use the computer's connection, with HDMI carrying the picture and sound to the television.

The visual design should support this setup without turning the home screen into a technical monitoring dashboard.

## Existing applications and an important change of direction

I already have two personal applications:

1. **Remote:** an existing Android remote-control application. A future integration may be considered, but mouse and keyboard use on the Linux computer define this design.
2. **Color Dock:** an existing, separate application already installed on the LG television. Leave it completely untouched. Mediacenter does not replace it or require changes to it.

**Mediacenter must have its own visual identity. I do not want it to resemble Color Dock or look like a desktop version of it.**

The earlier image-generated concepts were rejected. I now prefer **Colour Wall, direction 2 in `2026-11-05-mediacenter-visual-directions.html`**, with the corrections in this brief. This is a different second option from the earlier rejected image. Preserve Colour Wall's main idea: a personal-media column and a clear grid of eight streaming services, with strong typography and color.

The revised local visual reference is `2026-11-05-mediacenter-colour-wall-desktop.html`. It shows the selected structure with a theme switch and locally stored official service logos. The original four-direction file is historical reference; its TV-first controls and approximate logos are not requirements.

The existing applications matter because of how the devices might work together. They do not dictate Mediacenter's colors, layout, typography, navigation, wallpaper treatment, or component styling. Please approach its appearance as a fresh design problem.

## The services I want to access

The application needs entry points for these eight services:

1. Netflix
2. YouTube
3. HBO Max
4. Viaplay
5. MTV Katsomo
6. Ruutu
7. Yle Areena
8. Plex

Include all eight in the home-screen design. **Use actual official logo assets from the services' own websites or brand resources. Do not redraw logos with fonts, CSS, or generated imagery. Show only the logo on each service tile, with no additional visible repetition of the service name.** For example, a Netflix wordmark must not also have a separate Netflix caption. Preserve accessible names without displaying duplicate labels.

These would generally open the service's own browser interface or an appropriate installed application. You are designing the Mediacenter interface around that experience, not redesigning Netflix or the other services themselves. Actual service compatibility will be tested separately during development.

## Other capabilities the design should accommodate

Beyond opening streaming services, the broader application should have a sensible place for:

1. My own videos, including playback and resuming local videos.
2. My music and playback controls.
3. My photos and slideshows.
4. Browsing connected external hard drives and flash drives through USB-A or USB-C.
5. VPN connection status and choosing a connection or country.
6. Choosing computer or HDMI playback outputs while keeping the control interface on the computer.
7. A future optional Remote integration that does not dictate the desktop layout.
8. Returning focus to Mediacenter after using a streaming service or media player.

These are capabilities to organize, not a requirement to display every control on the home screen. Choose a clear hierarchy and decide what belongs in secondary views.

Please do not invent a universal streaming catalogue, combined recommendations, or shared viewing history across all eight services. We do not have those integrations. A continue-watching section for **local media** is a reasonable concept if clearly identified. Any sample content or connection status in a mockup is illustrative.

## How it will be used

1. The primary surface is a **normal Linux desktop application window on the computer's own display**. Support resizing and maximizing rather than imposing a fixed television canvas.
2. Design for mouse, trackpad, and keyboard. Include clear hover and keyboard-focus states, with normal desktop-sized controls.
3. The television is an optional HDMI playback output. The intended arrangement can use an extended desktop with controls on the computer and playback on the television. Do not assume screen mirroring.
4. The application must remain useful when the television is disconnected.
5. English interface text is fine for these mockups.

## What I want from you visually

Refine **Colour Wall** into a desktop design and show both **light and dark themes immediately**, using the same layout and functionality.

Keep the structure I liked, while adjusting spacing, density, and interaction cues for computer use. Retain the visual strength of the service grid and the personal-media column. The dark theme must be designed with readable text, useful contrast, and undistorted official logos.

**No dot separators anywhere.** Do not use middle dots, bullets, circular separators, decorative dots, or small status balls in text, navigation, labels, metadata, tooltips, settings, or connection indicators. Use spacing, separate fields, line breaks, or explicit wording instead. This applies to both themes and all future screens. Do not introduce dot-based loading indicators. Keep the original geometry of official brand assets intact; the rule forbids added UI decoration, not alteration of trademarks.

Service tiles must not repeat their logo's name underneath it. Remove the television-remote instruction strip and the large permanent selected-tile enlargement. Desktop hover and keyboard focus should be clear without overlapping neighboring tiles.

I care about a distinctive, polished result. Please use your visual judgment rather than simply arranging the feature list into boxes. Show deliberate spacing, readable typography, balanced imagery, and an understandable main action. The interface should feel like a personal entertainment application rather than a generic business dashboard.

## Requested output

For the light and dark versions:

1. Show a detailed desktop-window mockup of the same main screen.
2. Use actual official logo assets and logo-only service tiles.
3. Show a mouse-hover or keyboard-focus treatment without changing the grid geometry.
4. Keep all statuses and sample media clearly illustrative.

Use visual output, not just written descriptions or rough wireframes. If image generation is available, images are welcome. If your environment is better suited to a visual artifact, you may create a **static mockup presentation solely to display the designs**. It must remain a design illustration, not a working application.

Let me compare the two themes and request refinements before any application implementation begins.

## Scope boundary

For this request, do not:

1. Scaffold or implement the Mediacenter application.
2. Create a functional prototype, backend, database, or application routing.
3. Implement streaming, VPN, HDMI, file-access, or remote-control integrations.
4. Modify Remote or Color Dock.
5. Install dependencies, deploy anything, or create an implementation plan.
6. Treat my selection of a visual concept as permission to start building software.

**The deliverable is visual design help: the selected Colour Wall concept adapted for a Linux desktop application, in light and dark themes, with official logo-only tiles and no dot separators or status balls.**
