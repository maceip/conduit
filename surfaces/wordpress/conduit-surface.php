<?php
/**
 * Plugin Name: Conduit Surface
 * Plugin URI: https://github.com/maceip/conduit
 * Description: Publishes what this WordPress site knows to a Conduit client.
 * Version: 0.1.0
 * Requires at least: 6.0
 * Requires PHP: 7.4
 * Author: Conduit
 * License: MIT
 * License URI: https://opensource.org/licenses/MIT
 * Text Domain: conduit-surface
 *
 * @package Conduit
 */

if ( ! defined( 'ABSPATH' ) ) {
	exit;
}

require_once __DIR__ . '/NearEnd.php';

/**
 * The schema of the site itself.
 */
define( 'CONDUIT_SURFACE_SITE_SCHEMA', 'dev.conduit.wordpress.site/v1' );

/**
 * The schema of a post.
 */
define( 'CONDUIT_SURFACE_POST_SCHEMA', 'dev.conduit.wordpress.post/v1' );

/**
 * How long a publish waits for the tunnel to agree, in seconds.
 */
define( 'CONDUIT_SURFACE_TIMEOUT', 10 );

/**
 * The site is up: say what the site knows about itself.
 */
add_action( 'init', 'conduit_surface_publish_site' );

/**
 * A post has been written: say what it says.
 */
add_action( 'save_post', 'conduit_surface_publish_post', 10, 3 );

/**
 * A post has changed status: say it when it goes public.
 */
add_action( 'transition_post_status', 'conduit_surface_publish_transition', 10, 3 );

/**
 * The near end the surface publishes to.
 *
 * @return Conduit_NearEnd
 */
function conduit_surface_near_end() {
	static $near_end = null;
	if ( $near_end === null ) {
		$near_end = new Conduit_NearEnd(
			conduit_surface_socket_path(),
			conduit_surface_timeout()
		);
	}
	return $near_end;
}

/**
 * Where the near end is.
 *
 * The environment variable first, then the option of the site, and then
 * the default.
 *
 * @return string
 */
function conduit_surface_socket_path() {
	$path = getenv( 'CONDUIT_SOCKET' );
	if ( is_string( $path ) && $path !== '' ) {
		return $path;
	}
	$path = get_option( 'conduit_near_end_socket' );
	if ( is_string( $path ) && $path !== '' ) {
		return $path;
	}
	return '/tmp/conduit-wordpress.sock';
}

/**
 * How long a publish waits for the tunnel to agree, in seconds.
 *
 * A site that would rather answer its visitors quickly than wait for a
 * tunnel can make this smaller with the filter.
 *
 * @return float
 */
function conduit_surface_timeout() {
	return (float) apply_filters( 'conduit_surface_timeout', CONDUIT_SURFACE_TIMEOUT );
}

/**
 * Publish something, and say so when it did not work.
 *
 * A site must not break because the near end is not running, so a refusal
 * is written to the log and the request carries on.
 *
 * @param string $schema  The schema of the payload.
 * @param array  $payload The payload.
 */
function conduit_surface_publish( $schema, array $payload ) {
	try {
		conduit_surface_near_end()->publish( $schema, $payload );
	} catch ( Exception $error ) {
		error_log( 'conduit: the surface was not published: ' . $error->getMessage() );
	}
}

/**
 * Publish what the site knows about itself.
 */
function conduit_surface_publish_site() {
	$payload = array(
		'siteTitle'   => (string) get_bloginfo( 'name' ),
		'siteUrl'     => home_url( '/' ),
		'description' => (string) get_bloginfo( 'description' ),
		'locale'      => (string) get_locale(),
		'timezone'    => (string) wp_timezone_string(),
		'publishedAt' => gmdate( 'c' ),
	);
	conduit_surface_publish( CONDUIT_SURFACE_SITE_SCHEMA, $payload );
}

/**
 * Publish the post that has just been written.
 *
 * @param int     $post_id The post.
 * @param WP_Post $post    The post.
 * @param bool    $update  Whether this was an update.
 */
function conduit_surface_publish_post( $post_id, $post, $update ) {
	if ( ! $post instanceof WP_Post ) {
		return;
	}
	if ( $post->post_status === 'auto-draft' || $post->post_type === 'revision' ) {
		return;
	}
	conduit_surface_publish( CONDUIT_SURFACE_POST_SCHEMA, conduit_surface_post_payload( $post ) );
}

/**
 * Publish the post whose status has just changed.
 *
 * @param string  $new_status The status it is in now.
 * @param string  $old_status The status it was in.
 * @param WP_Post $post       The post.
 */
function conduit_surface_publish_transition( $new_status, $old_status, $post ) {
	if ( $new_status !== 'publish' || $old_status === 'publish' ) {
		return;
	}
	if ( ! $post instanceof WP_Post ) {
		return;
	}
	conduit_surface_publish( CONDUIT_SURFACE_POST_SCHEMA, conduit_surface_post_payload( $post ) );
}

/**
 * What is said about a post.
 *
 * @param WP_Post $post The post.
 *
 * @return array
 */
function conduit_surface_post_payload( $post ) {
	$permalink = get_permalink( $post );
	return array(
		'postId'      => (int) $post->ID,
		'title'       => (string) $post->post_title,
		'status'      => (string) $post->post_status,
		'type'        => (string) $post->post_type,
		'url'         => $permalink === false ? '' : (string) $permalink,
		'author'      => (string) get_the_author_meta( 'display_name', (int) $post->post_author ),
		'publishedAt' => $post->post_date_gmt . 'Z',
		'excerpt'     => conduit_surface_excerpt( $post ),
	);
}

/**
 * The first words of a post, without the markup.
 *
 * @param WP_Post $post The post.
 *
 * @return string
 */
function conduit_surface_excerpt( $post ) {
	$words = wp_trim_words( wp_strip_all_tags( $post->post_content ), 40 );
	return is_string( $words ) ? $words : '';
}
