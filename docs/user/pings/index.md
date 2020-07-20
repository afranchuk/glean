# Glean Pings

Every Glean ping is in JSON format and contains one or more of the [common sections](#ping-sections) with shared information data.

If data collection is enabled, the Glean SDK provides a set of built-in pings that are assembled out of the box without any developer intervention.  The following is a list of these built-in pings:

- [`baseline` ping](baseline.md): A small ping sent every time the application goes to foreground and background. Going to foreground also includes when the application starts.
- [`metrics` ping](metrics.md): The default ping for metrics. Sent approximately daily.
- [`events` ping](events.md): The default ping for events. Sent every time the application goes to background or a certain number of events is reached.
- [`deletion-request` ping](deletion_request.md): Sent when the user disables telemetry in order to request a deletion of their data.

Applications can also define and send their own [custom pings](custom.md) when the schedules of these pings is not suitable.

There is also a [high-level overview](ping-schedules-and-timings.html) of how the `metrics` and `baseline` pings relate and the timings they record.

## Ping sections

There are two standard metadata sections that are added to most pings, in addition to their core metrics and events content (which are described in [Adding new metrics](../adding-new-metrics.md)).

- The [`ping_info` section](#The-ping_info-section) contains core metadata that is included in **every** ping.
  
- The [`client_info` section](#The-client_info-section) contains information that identifies the client.
  It is included in most pings (including all built-in pings), but may be excluded from pings where we don't want to connect client information with the other metrics in the ping.

### The `ping_info` section
The following fields are included in the `ping_info` section, for every ping.
Optional fields are marked accordingly.

| Field name | Type | Description |
|---|---|---|
| `seq` | Counter | A running counter of the number of times pings of this type have been sent |
| `experiments` | Object | *Optional*. A dictionary of [active experiments](#the-experiments-object) |
| `start_time` | Datetime | The time of the start of collection of the data in the ping, in local time and with minute precision, including timezone information. |
| `end_time` | Datetime | The time of the end of collection of the data in the ping, in local time and with minute precision, including timezone information. This is also the time this ping was generated and is likely well before ping transmission time. |
| `reason` | String | *Optional*. The reason the ping was submitted. The specific set of values and their meanings are defined for each metric type in the `reasons` field in the `pings.yaml` file. |

All the metrics surviving application restarts (e.g. `seq`, ...) are removed once the application using the Glean SDK is uninstalled.

### The `client_info` section
The following fields are included in the `client_info` section.
Optional fields are marked accordingly.

| Field name | Type | Description |
|---|---|---|
| `app_build` | String | The build identifier generated by the CI system (e.g. "1234/A"). For language bindings that provide automatic detection for this value, (e.g. Android/Kotlin), in the unlikely event that the build identifier can not be retrieved from the OS, it is set to `inaccessible`. For other language bindings, if the value was not provided through configuration, this metric gets set to `Unknown`. |
| `app_channel` | String | *Optional* The product-provided release channel (e.g. "beta") |
| `app_display_version` | String | The user-visible version string (e.g. "1.0.3"). In the unlikely event this value can not be obtained from the OS, it is set to "inaccessible". If it is accessible, but not set by the application, it is set to "Unknown". |
| `architecture` | String | The architecture of the device (e.g. "arm", "x86") |
| `client_id` | UUID |  *Optional* A UUID identifying a profile and allowing user-oriented correlation of data |
| `device_manufacturer` | String | *Optional* The manufacturer of the device |
| `device_model` | String | *Optional* The model name of the device. On Android, this is [`Build.MODEL`], the user-visible name of the device. |
| `first_run_date` | Datetime | The date of the first run of the application, in local time and with day precision, including timezone information. |
| `os` | String | The name of the operating system (e.g. "linux", "Android", "ios") |
| `os_version` | String | The user-visible version of the operating system (e.g. "1.2.3") |
| `android_sdk_version` | String | *Optional*. The Android specific SDK version of the software running on this hardware device (e.g. "23") |
| `telemetry_sdk_build` | String | The version of the Glean SDK |
| `locale` | String | *Optional*. The locale of the application during initialization (e.g. "es-ES"). If the locale can't be determined on the system, the value is "und", to indicate "undetermined". |

All the metrics surviving application restarts (e.g. `client_id`, ...) are removed once the application using the Glean SDK is uninstalled.

[`Build.MODEL`]: https://developer.android.com/reference/android/os/Build.html#MODEL

### The `experiments` object

This object (included in the [`ping_info` section](#The-ping_info-section)) contains experiments keyed by the experiment `id`. Each listed experiment contains the `branch` the client is enrolled in and may contain a string to string map with additional data in the `extra` key. Both the `id` and `branch` are truncated to 30 characters.
See [Using the Experiments API](../experiments-api.md) on how to record experiments data.

```json
{
  "<id>": {
    "branch": "branch-id",
    "extra": {
      "some-key": "a-value"
    }
  }
}
```

## Ping submission

The pings that the Glean SDK generates are submitted to the Mozilla servers at specific paths, in order to provide additional metadata without the need to unpack the ping payload.

> **Note**: To keep resource usage in check, the Glean SDK allows only up to 10 ping submissions every 60 seconds. There are no exposed methods to change these rate limiting defaults yet, follow [Bug 1647630](https://bugzilla.mozilla.org/show_bug.cgi?id=1647630) for updates.

A typical submission URL looks like

  `"<server-address>/submit/<application-id>/<doc-type>/<glean-schema-version>/<document-id>"`

where:

- `<server-address>`: the address of the server that receives the pings;
- `<application-id>`: a unique application id, automatically detected by the Glean SDK; this is the value returned by [`Context.getPackageName()`](http://developer.android.com/reference/android/content/Context.html#getPackageName());
- `<doc-type>`: the name of the ping; this can be one of the pings available out of the box with the Glean SDK, or a custom ping;
- `<glean-schema-version>`: the version of the Glean ping schema;
- `<document-id>`: a unique identifier for this ping.

### Submitted headers
A pre-defined set of headers is additionally sent along with the submitted ping:

| Header | Value | Description |
|--------|-------|-------------|
| `Content-Type` | `application/json; charset=utf-8` | Describes the data sent to the server |
| `User-Agent` | Defaults to e.g. `Glean/0.40.0 (Kotlin on Android)`, where `0.40.0` is the Glean SDK version number and `Kotlin on Android` is the name of the language used by the binding that sent the request plus the name of the platform it is running on. | Describes the application sending the ping using the Glean SDK |
| `Date` | e.g. `Mon, 23 Jan 2019 10:10:10 GMT+00:00` | Submission date/time in GMT/UTC+0 offset |
| `X-Client-Type` | `Glean` | Custom header to support handling of Glean pings in the legacy pipeline |
| `X-Client-Version` | e.g. `0.40.0` | The Glean SDK version, sent as a custom header to support handling of Glean pings in the legacy pipeline |


## Defining foreground and background state

These docs refer to application 'foreground' and 'background' state in several places.

{{#include ../../tab_header.md}}

<div data-lang="Kotlin" class="tab">

#### Foreground

For Android, this specifically means the activity becomes visible to the user, it has entered the `Started` state, and the system invokes the [`onStart()`](https://developer.android.com/reference/android/app/Activity.html#onStart()) callback.

### Background

This specifically means when the activity is no longer visible to the user, it has entered the `Stopped` state, and the system invokes the [`onStop()`](https://developer.android.com/reference/android/app/Activity.html#onStop()) callback.

This may occur, if the user uses `Overview` button to change to another app, the user presses the `Back` button and
navigates to a previous application or the home screen, or if the user presses the `Home` button to return to the
home screen.  This can also occur if the user navigates away from the application through some notification or
other means.

The system may also call `onStop()` when the activity has finished running, and is about to be terminated.

</div>

<div data-lang="Swift" class="tab">

### Foreground

For iOS, Glean attaches to the [`willEnterForegroundNotification`](https://developer.apple.com/documentation/uikit/uiapplication/1622944-willenterforegroundnotification).
This notification is posted by the OS shortly before an app leaves the background state on its way to becoming the active app.

### Background

For iOS, this specifically means when the app is no longer visible to the user, or when the `UIApplicationDelegate`
receives the [`applicationDidEnterBackground`](https://developer.apple.com/documentation/uikit/uiapplicationdelegate/1622997-applicationdidenterbackground) event.

This may occur if the user opens the task switcher to change to another app, or if the user presses the `Home` button
to show the home screen.  This can also occur if the user navigates away from the app through a notification or other
means.

> **Note:** Glean does not currently support [Scene based lifecycle events](https://developer.apple.com/documentation/uikit/app_and_environment/managing_your_app_s_life_cycle) that were introduced in iOS 13.

</div>

{{#include ../../tab_footer.md}}