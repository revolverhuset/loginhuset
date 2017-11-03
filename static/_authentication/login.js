window.onload = () => {
    const form = document.forms['login'];
    let contents = document.querySelector('.contents');
    form.onsubmit = function(e) {
        e.preventDefault();
        fetch('/_authentication/authenticate'+(window.location.search||''), {
            method: 'POST',
            body: `email=${form.elements['email'].value}`
        }).then(function(response) {
            if (response.status == 200) {
                contents.innerText = 'Epost underveis, om epost-adressen er kjent';
            } else {
                throw Error(response.status);
            }
        }).catch(function(error) {
            contents.innerText = 'Beklager, det har oppstått en feil. Prøv kaffiknappen.';
            console.error(error);
        });
    };
};
