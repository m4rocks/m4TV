# App logos

The Home Tiles show each App's official, unmodified logo (ADR 0005), one file per theme:

    <app id>-dark.png    for the Dark theme (a light logo)
    <app id>-light.png   for the Light theme (a dark logo)

The ids are the built-in Apps': `youtube`, `netflix`, `spotify`. A transparent PNG, about 480 px
wide. The Shell embeds whatever is here at build time; an App without a file shows its name instead.

The artwork is the services' trademark. Take it from each service's brand page and keep it as is.
