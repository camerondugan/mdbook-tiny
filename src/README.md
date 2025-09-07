# mdbook-tiny

An alternative renderer that generates tiny and performant html from your mdbook.

Currently delivers a 100 on lighthouse speed score vs around 85 from default mdbook renderer.

The CSS file you specify is inlined into the html, making your website render one round trip.

Most of my pages now [load in under 14.kb](https://endtimes.dev/why-your-website-should-be-under-14kb-in-size/)
