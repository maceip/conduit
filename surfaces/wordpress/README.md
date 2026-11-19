# The WordPress surface

A WordPress plugin is, at its simplest, a PHP file with a WordPress plugin
header comment in it: the header is what WordPress reads to know the name
of the plugin, its version and who wrote it, and the minimum of it is the
`Plugin Name:` line.  Everything else a plugin does is done through hooks:
`add_action` to say what happens when something happens, `add_filter` to
change what something is, and `do_action` to say something happened
yourself.

This plugin registers three hooks:

| the hook | what it publishes |
|---|---|
| `init` | what the site knows about itself: its title, its address, its description, its language and its timezone |
| `save_post` | the post that has just been written |
| `transition_post_status` | the post, again, when it goes public |

| the part | the file |
|---|---|
| the declaration | `surface.json` |
| the plugin | `conduit-surface.php` |
| the client of the near end | `NearEnd.php` |

## What it publishes

The site under the schema `dev.conduit.wordpress.site/v1`, and each post
under `dev.conduit.wordpress.post/v1`.  A post is its id, its title, its
status, its type, its address, its author, when it was published and the
first forty words of it.

The plugin publishes in plain JSON to the near end.  It holds no key, it
opens no socket to the broker, and it seals nothing: the near end does the
sealing, and the broker only ever sees the sealed form.

Before the first publish it waits for the tunnel to agree its keys,
because the near end cannot seal anything until the two ends have met.
It asks `{"op":"status"}` and waits for `{"op":"status",…,"agreed":true}`,
for up to ten seconds; a publish that is sent before that is refused with
`{"op":"error"}`.  A site that would rather answer its visitors quickly
than wait for a tunnel can shorten the wait with the
`conduit_surface_timeout` filter:

```php
add_filter( 'conduit_surface_timeout', fn() => 2 );
```

## What the user has to do

1. **Install it.**  Put the two files in a directory called
   `conduit-surface` under `wp-content/plugins/`, so that the layout is
   `wp-content/plugins/conduit-surface/conduit-surface.php` and
   `wp-content/plugins/conduit-surface/NearEnd.php`, and zip that
   directory.  WordPress wants the directory of a plugin to be named after
   the plugin, and the main file of the plugin to be named after the
   directory.
2. **Turn it on.**  Plugins → Installed Plugins → *Conduit Surface* →
   Activate.
3. **Point it at the near end.**  The plugin looks for the socket in
   `CONDUIT_SOCKET`, then in the option `conduit_near_end_socket`, and
   then at `/tmp/conduit-wordpress.sock`.  The near end must be running
   with the same path.
4. **Give it something to say.**  Write or publish a post, and the client
   at the far end receives the site and the post, opened.

## What was verified

**PHP is not installed on this machine, so the plugin was never parsed and
never run.**  `conduit-surface.php` and `NearEnd.php` have never been
seen by a PHP interpreter, no WordPress has ever loaded the plugin, and
no hook has ever fired.  What was written was written by hand against
WordPress's documented plugin header and hook API, and the protocol
against the contract in `docs/surface-contract.md` and the Node client in
`clients/native/near-end.js`.

To run it, on a machine with PHP and WordPress:

* `php -l conduit-surface.php` and `php -l NearEnd.php` to parse the two
  files,
* copy them into `wp-content/plugins/conduit-surface/`, activate the
  plugin, and publish a post with a near end running,
* and write a `check.mjs` in this directory that drives the *same*
  publishing code against a live broker and a live near end, the way
  `surfaces/vscode/check.mjs` does.  There is no such check here: it
  would have to run the PHP, and PHP cannot run here.
