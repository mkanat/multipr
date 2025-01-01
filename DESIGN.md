This file contains the basic plans for the system, as well as some principles we follow.

== Principles ==

* Never modify any existing file (including the history of any repository) unless the user has said yes at a prompt or provided a flag that indicates that intention.
* Don't be surprising. By default, behave the same as other tools the developer uses for the same task we are doing. For example, if we are doing a git diff, by default behave exactly like `git diff`.
* Try to constrain the UI surface of any individual tool. One common problem with developer CLI tools is that they explode with flags that almost nobody ever needs. Before you add a flag or a UI element, think, "How often does the user really need this? If it's uncommon, could they just use some external tool to do it instead?" For example, we don't provide the option to use patience diff when we are diffing a git repo, because if you _really_ needed to, you could just do `git diff --patience` yourself, and we don't think it will be needed that often. Provide flags and UI elements for the choices and actions people frequently _need_ to make.

== Design ==

Multipr consists of a few different components:

1. A system for identifying files that you want to change across one repo or many repos.
2. A system for running transformations across one repo or many repos.
3. A tool for splitting PRs or diffs or git commits or [your form of code change here] into multiple PRs or code reviews.
4. A tool for sending out lots of code reviews, and assigning them to reviewers.
5. A tool for tracking the status of a set of code reviews, including everything needed to make it easy to manage hundreds or thousands of parallel code reviews all with the same author (presumably because the author used automation to generate the changes)

The initial plan is to support GitHub, but it should be possible to support other code review tools with minor modifications. If there are requests for more code review tools specifically, that's when we will refactor to support multiple tools via configuration or plugins or whatever we decide.

We also intend to provide some of this functionality as a library so that other tools can integrate.

I would like to learn AWS, including the serverless components of it, so anything that is a server/API will be available both as a standalone server and in a form suitable for running in a serverless environment.

My normal philosophy in building software is to start from an essential core that is useful even if it's not very good yet. Each of the three parts above is potentially useful on its own. Parts 1 and 2 can be severed from the system and be their own product, and so don't have to be built first. Parts 3 - 5 are all part of the same system, but Part 3 is required for the other two to be truly useful, and so we will start with building Part 3.