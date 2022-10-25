// FIXME MANUAL HACK material-components/material-components-web#7618
mdc.list.MDCList.prototype.handleClickEvent = function (evt) {
    var index = this.getListItemIndex(evt.target);
    var target = evt.target;
    // Toggle the checkbox only if it's not the target of the event, or the checkbox will have 2 change events.
    var isCheckboxAlreadyUpdatedInAdapter = mdc.dom.ponyfill.matches(target, mdc.list.strings.CHECKBOX_RADIO_SELECTOR);
    this.foundation.handleClick(index, isCheckboxAlreadyUpdatedInAdapter, evt);
};
